use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

use crate::agent_config::AgentConfig;
use crate::llm::{extract_text, ContentItem, Message, ModelProvider, ProviderRequest, StreamEvent};
use crate::memory::TapeStore;
use crate::runtime::RuntimeProfile;
use crate::tool_view::ProgressiveToolView;
use crate::tools::{make_tool_message, ToolRegistry};

pub struct AgentLoop<P: ModelProvider> {
    provider: P,
    tools: ToolRegistry,
    tape: TapeStore,
    workspace: PathBuf,
    tool_view: ProgressiveToolView,
    profile: RuntimeProfile,
    config: AgentConfig,
}

impl<P: ModelProvider> AgentLoop<P> {
    pub fn new(
        provider: P,
        tools: ToolRegistry,
        tape: TapeStore,
        workspace: PathBuf,
        profile: RuntimeProfile,
        config: AgentConfig,
    ) -> Self {
        let tool_view = ProgressiveToolView::new(tools.specs());
        Self {
            provider,
            tools,
            tape,
            workspace,
            tool_view,
            profile,
            config,
        }
    }

    pub fn handle_input(&mut self, input: &str) -> Result<String, String> {
        let trimmed = input.trim();

        if let Some(rest) = trimmed.strip_prefix('!') {
            return if self.profile.shell_route_allowed() {
                self.run_shell(rest.trim())
            } else {
                Ok("shell route disabled for current profile".to_string())
            };
        }

        if let Some(rest) = trimmed.strip_prefix('/') {
            let mut parts = rest.splitn(2, ' ');
            let command = parts.next().map(str::trim).unwrap_or("help");
            let args = parts.next().map(str::trim).unwrap_or_default();
            return self.handle_command(command, args);
        }

        self.run_turn(input)
    }

    fn run_turn(&mut self, input: &str) -> Result<String, String> {
        let user_msg = Message {
            role: "user".to_string(),
            content: vec![ContentItem::Text(input.to_string())],
        };
        self.tape.append_message(&user_msg)?;

        let mut messages = self
            .tape
            .build_messages(self.config.limits.context_window)?;
        messages.insert(
            0,
            Message {
                role: "system".to_string(),
                content: vec![ContentItem::Text(self.config.system_prompt.to_string())],
            },
        );

        let tools = self
            .tool_view
            .specs()
            .iter()
            .map(|t| t.name.clone())
            .collect::<Vec<_>>()
            .join(",");
        self.tape.append_event(
            "turn_start",
            &[
                (
                    "prompt_version".to_string(),
                    self.config.prompt_version.to_string(),
                ),
                ("profile".to_string(), self.profile.name().to_string()),
                ("toolset".to_string(), tools),
            ],
        )?;

        let mut seen_tool_calls = HashSet::new();

        for round in 0..self.config.limits.max_rounds {
            self.tool_view.activate_hints(input);
            let visible_tools = self.tool_view.specs();

            self.tape.append_event(
                "round_start",
                &[
                    ("round".to_string(), round.to_string()),
                    (
                        "prompt_version".to_string(),
                        self.config.prompt_version.to_string(),
                    ),
                    (
                        "toolset".to_string(),
                        visible_tools
                            .iter()
                            .map(|t| t.name.clone())
                            .collect::<Vec<_>>()
                            .join(","),
                    ),
                ],
            )?;

            let stream = self.provider.stream_generate(ProviderRequest {
                messages: messages.clone(),
                tools: visible_tools.clone(),
            });
            self.record_stream_events(round, &stream)?;

            let response = self.provider.generate(ProviderRequest {
                messages: messages.clone(),
                tools: visible_tools,
            });
            self.tape.append_event(
                "provider_usage",
                &[
                    ("provider".to_string(), response.provider_id),
                    ("stop_reason".to_string(), response.stop_reason),
                    (
                        "input_tokens".to_string(),
                        response.usage.input_tokens.to_string(),
                    ),
                    (
                        "output_tokens".to_string(),
                        response.usage.output_tokens.to_string(),
                    ),
                    (
                        "total_tokens".to_string(),
                        response.usage.total_tokens.to_string(),
                    ),
                ],
            )?;

            let assistant = response.message;
            self.tape.append_message(&assistant)?;
            messages.push(assistant.clone());

            let tool_calls = collect_tool_calls(&assistant);
            if tool_calls.is_empty() {
                self.tape.append_event(
                    "turn_stop",
                    &[
                        ("reason".to_string(), "assistant_final".to_string()),
                        ("round".to_string(), round.to_string()),
                    ],
                )?;
                return Ok(extract_text(&assistant));
            }

            for (idx, (call_id, name, args)) in tool_calls.iter().enumerate() {
                if idx >= self.config.limits.max_tool_calls_per_round {
                    self.tape.append_event(
                        "tool_call_skipped",
                        &[
                            ("reason".to_string(), "tool_call_cap".to_string()),
                            (
                                "limit".to_string(),
                                self.config.limits.max_tool_calls_per_round.to_string(),
                            ),
                        ],
                    )?;
                    break;
                }

                let sig = format!("{}:{:?}", name, args);
                if seen_tool_calls.contains(&sig) {
                    self.tape.append_event(
                        "tool_call",
                        &[
                            ("tool".to_string(), name.clone()),
                            ("call_id".to_string(), call_id.clone()),
                            ("status".to_string(), "duplicate".to_string()),
                        ],
                    )?;
                    continue;
                }
                seen_tool_calls.insert(sig);
                self.tool_view.note_selected(name);

                let result = self.tools.execute(&self.workspace, name, args);
                match result {
                    Ok(result) => {
                        self.tape.append_event(
                            "tool_call",
                            &[
                                ("tool".to_string(), name.clone()),
                                ("call_id".to_string(), call_id.clone()),
                                ("round".to_string(), round.to_string()),
                                ("status".to_string(), "ok".to_string()),
                                ("args".to_string(), format!("{:?}", args)),
                                ("ui_details".to_string(), format!("{:?}", result.ui_details)),
                            ],
                        )?;
                        let tool_msg = make_tool_message(&result);
                        self.tape.append_message(&tool_msg)?;
                        messages.push(tool_msg);
                    }
                    Err(err) => {
                        self.tape.append_event(
                            "tool_call",
                            &[
                                ("tool".to_string(), name.clone()),
                                ("call_id".to_string(), call_id.clone()),
                                ("round".to_string(), round.to_string()),
                                ("status".to_string(), "error".to_string()),
                                ("error".to_string(), err),
                            ],
                        )?;
                    }
                }
            }
        }

        self.tape.append_event(
            "turn_stop",
            &[("reason".to_string(), "max_rounds".to_string())],
        )?;
        Ok(format!(
            "stopped after {} rounds",
            self.config.limits.max_rounds
        ))
    }

    fn record_stream_events(&self, round: usize, stream: &[StreamEvent]) -> Result<(), String> {
        for (index, event) in stream.iter().enumerate() {
            match event {
                StreamEvent::TextDelta(delta) => self.tape.append_event(
                    "assistant_text_delta",
                    &[
                        ("round".to_string(), round.to_string()),
                        ("index".to_string(), index.to_string()),
                        ("delta".to_string(), delta.clone()),
                    ],
                )?,
                StreamEvent::ToolCallDelta { name, partial_args } => self.tape.append_event(
                    "tool_args_partial",
                    &[
                        ("round".to_string(), round.to_string()),
                        ("index".to_string(), index.to_string()),
                        ("tool".to_string(), name.clone()),
                        ("partial".to_string(), partial_args.clone()),
                    ],
                )?,
                StreamEvent::Done(usage) => self.tape.append_event(
                    "assistant_stream_done",
                    &[
                        ("round".to_string(), round.to_string()),
                        ("index".to_string(), index.to_string()),
                        ("total_tokens".to_string(), usage.total_tokens.to_string()),
                    ],
                )?,
            }
        }
        Ok(())
    }

    fn run_shell(&self, command: &str) -> Result<String, String> {
        if command.trim().is_empty() {
            return Ok("empty shell command".to_string());
        }
        let out = Command::new("/usr/bin/timeout")
            .arg("30")
            .arg("/bin/sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.workspace)
            .output()
            .map_err(|e| e.to_string())?;

        let mut text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        if !err.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&err);
        }
        Ok(text)
    }

    fn handle_command(&self, command: &str, args: &str) -> Result<String, String> {
        match command {
            "help" => Ok("/help\n/tools\n/trace\n/handoff <name>\n!<cmd>".to_string()),
            "tools" => Ok(self
                .tools
                .specs()
                .iter()
                .map(|s| format!("{}: {}", s.name, s.description))
                .collect::<Vec<_>>()
                .join("\n")),
            "trace" => {
                let entries = self.tape.entries()?;
                let lines = entries
                    .iter()
                    .rev()
                    .take(20)
                    .map(|e| format!("#{} [{}] {}", e.id, e.entry_type, e.payload))
                    .collect::<Vec<_>>();
                Ok(lines.into_iter().rev().collect::<Vec<_>>().join("\n"))
            }
            "handoff" => {
                self.tape
                    .append_event("handoff", &[("name".to_string(), args.trim().to_string())])?;
                Ok("handoff saved".to_string())
            }
            _ => Ok("unknown command".to_string()),
        }
    }
}

fn collect_tool_calls(message: &Message) -> Vec<(String, String, Vec<(String, String)>)> {
    message
        .content
        .iter()
        .filter_map(|item| match item {
            ContentItem::ToolCall {
                id,
                name,
                arguments,
            } => Some((id.clone(), name.clone(), arguments.clone())),
            _ => None,
        })
        .collect()
}
