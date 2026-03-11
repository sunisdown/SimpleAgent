use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

use crate::llm::{extract_text, ContentItem, Message, ModelProvider};
use crate::memory::TapeStore;
use crate::router::{RouteKind, Router};
use crate::tool_view::ProgressiveToolView;
use crate::tools::{make_tool_result, AgentToolResult, ToolRegistry};

const SYSTEM_PROMPT_VERSION: &str = "v1";
const SYSTEM_PROMPT_TEXT: &str = "You are SimpleAgent. Be concise, deterministic, and observable. Prefer tool calls when needed, and avoid hidden assumptions.";

pub struct AgentLoop<P: ModelProvider> {
    provider: P,
    tools: ToolRegistry,
    tape: TapeStore,
    workspace: PathBuf,
    router: Router,
    max_rounds: usize,
    context_window: usize,
    max_tool_calls_per_round: usize,
    max_stalled_rounds: usize,
    tool_view: ProgressiveToolView,
}

impl<P: ModelProvider> AgentLoop<P> {
    pub fn new(provider: P, tools: ToolRegistry, tape: TapeStore, workspace: PathBuf) -> Self {
        let tool_view = ProgressiveToolView::new(tools.specs());
        Self {
            provider,
            tools,
            tape,
            workspace,
            router: Router,
            max_rounds: 15,
            context_window: 50,
            max_tool_calls_per_round: 6,
            max_stalled_rounds: 3,
            tool_view,
        }
    }

    pub fn handle_input(&mut self, text: &str) -> Result<String, String> {
        let route = self.router.route(text);
        if route.kind == RouteKind::Command {
            return self.handle_command(route.command.as_deref().unwrap_or("help"), &route.args);
        }

        let user = Message {
            role: "user".to_string(),
            content: vec![ContentItem::Text(text.to_string())],
        };
        self.tape.append_message(&user)?;

        let mut messages = self.tape.build_messages(self.context_window)?;
        messages.insert(
            0,
            Message {
                role: "system".to_string(),
                content: vec![ContentItem::Text(SYSTEM_PROMPT_TEXT.to_string())],
            },
        );

        let visible_tools = self.tool_view.specs();
        self.tape.append_event_json(
            "turn_start",
            &[
                (
                    "prompt_version".to_string(),
                    SYSTEM_PROMPT_VERSION.to_string(),
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

        let mut seen_calls: HashSet<String> = HashSet::new();
        let mut previous_plan: Option<String> = None;
        let mut repeated_plan_rounds = 0usize;
        let mut stalled_rounds = 0usize;

        for round in 0..self.max_rounds {
            self.tool_view.activate_hints(text);
            let round_tools = self.tool_view.specs();
            self.tape.append_event_json(
                "round_start",
                &[
                    ("round".to_string(), round.to_string()),
                    (
                        "prompt_version".to_string(),
                        SYSTEM_PROMPT_VERSION.to_string(),
                    ),
                    (
                        "toolset".to_string(),
                        round_tools
                            .iter()
                            .map(|t| t.name.clone())
                            .collect::<Vec<_>>()
                            .join(","),
                    ),
                ],
            )?;
            let assistant = self.provider.generate(&messages, &round_tools);
            self.tape.append_message(&assistant)?;
            messages.push(assistant.clone());

            let tool_calls = collect_tool_calls(&assistant);
            if tool_calls.is_empty() {
                self.tape.append_event_json(
                    "turn_stop",
                    &[
                        ("reason".to_string(), "assistant_final".to_string()),
                        ("round".to_string(), round.to_string()),
                    ],
                )?;
                return Ok(extract_text(&assistant));
            }

            let plan = signature_for_plan(&tool_calls);
            if previous_plan.as_deref() == Some(plan.as_str()) {
                repeated_plan_rounds += 1;
            } else {
                repeated_plan_rounds = 0;
            }
            previous_plan = Some(plan);

            if repeated_plan_rounds >= 2 {
                self.tape.append_event_json(
                    "turn_stop",
                    &[("reason".to_string(), "repeated_plan".to_string())],
                )?;
                return Ok(
                    "Stopped: the model repeated the same tool plan in consecutive rounds."
                        .to_string(),
                );
            }

            let mut succeeded_calls = 0usize;
            let mut attempted_calls = 0usize;

            for (idx, (call_id, tool_name, args)) in tool_calls.iter().enumerate() {
                if idx >= self.max_tool_calls_per_round {
                    let limit_result = AgentToolResult {
                        llm_output: format!(
                            "Tool plan exceeded per-round limit ({}). Remaining calls were skipped.",
                            self.max_tool_calls_per_round
                        ),
                        ui_details: vec![("status".to_string(), "limit_exceeded".to_string())],
                    };
                    let message = make_tool_result("loop_guard", "system", &limit_result);
                    self.tape.append_message(&message)?;
                    messages.push(message);
                    break;
                }

                attempted_calls += 1;
                let call_signature = format!("{}:{:?}", tool_name, args);
                let (tool_result, status) = if seen_calls.contains(&call_signature) {
                    (
                        AgentToolResult {
                            llm_output: format!(
                                "Skipped duplicate tool call for this turn history: {} {:?}",
                                tool_name, args
                            ),
                            ui_details: vec![("status".to_string(), "duplicate".to_string())],
                        },
                        "duplicate",
                    )
                } else {
                    seen_calls.insert(call_signature);
                    self.tool_view.note_selected(tool_name);
                    match self.tools.execute(&self.workspace, tool_name, args) {
                        Ok(result) => {
                            succeeded_calls += 1;
                            (result, "ok")
                        }
                        Err(err) => (
                            AgentToolResult {
                                llm_output: err,
                                ui_details: vec![("status".to_string(), "error".to_string())],
                            },
                            "error",
                        ),
                    }
                };

                let tool_message = make_tool_result(call_id, tool_name, &tool_result);
                self.tape.append_message(&tool_message)?;
                self.tape.append_event_json(
                    "tool_call",
                    &[
                        ("tool".to_string(), tool_name.clone()),
                        ("status".to_string(), status.to_string()),
                        ("args".to_string(), format!("{:?}", args)),
                        (
                            "ui_details".to_string(),
                            format!("{:?}", tool_result.ui_details),
                        ),
                    ],
                )?;
                messages.push(tool_message);
            }

            let was_stalled = attempted_calls == 0 || succeeded_calls == 0;
            if was_stalled {
                stalled_rounds += 1;
            } else {
                stalled_rounds = 0;
            }

            if stalled_rounds >= self.max_stalled_rounds {
                self.tape.append_event_json(
                    "turn_stop",
                    &[
                        ("reason".to_string(), "stalled_rounds".to_string()),
                        ("count".to_string(), stalled_rounds.to_string()),
                    ],
                )?;
                return Ok(format!(
                    "Stopped after {} stalled tool rounds (no successful calls).",
                    stalled_rounds
                ));
            }
        }

        self.tape.append_event_json(
            "turn_stop",
            &[
                ("reason".to_string(), "max_rounds".to_string()),
                ("max_rounds".to_string(), self.max_rounds.to_string()),
            ],
        )?;
        Ok(format!(
            "Tool-calling loop reached max rounds ({}).",
            self.max_rounds
        ))
    }

    fn handle_command(&mut self, command: &str, args: &str) -> Result<String, String> {
        match command {
            "help" | "h" => Ok(
                "Commands:\n/help\n/tools\n/tape.search <query>\n/handoff [name]\n!<shell command>"
                    .to_string(),
            ),
            "tools" => Ok(self
                .tools
                .specs()
                .iter()
                .map(|s| format!("- {}: {}", s.name, s.description))
                .collect::<Vec<_>>()
                .join("\n")),
            "tape.search" => {
                let results = self.tape.search(args)?;
                if results.is_empty() {
                    return Ok("No tape entries matched.".to_string());
                }
                Ok(results
                    .iter()
                    .take(20)
                    .map(|r| format!("#{} [{}] {}", r.id, r.entry_type, r.payload))
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
            "handoff" => {
                let before = self.tape.entries()?.len();
                let name = if args.trim().is_empty() {
                    format!("handoff-{before}")
                } else {
                    args.trim().to_string()
                };
                self.tape
                    .append_anchor(&name, &format!("entries_before={before}"), "handoff")?;
                Ok(format!(
                    "Handoff anchor '{name}' created. Context window reset ({before} entries before)."
                ))
            }
            "shell" => self.run_shell(args),
            _ => Ok(format!("Unknown command: {command}. Try /help.")),
        }
    }

    fn run_shell(&self, command: &str) -> Result<String, String> {
        if command.trim().is_empty() {
            return Ok("Empty command.".to_string());
        }

        let output = Command::new("/usr/bin/timeout")
            .arg("30")
            .arg("/bin/sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.workspace)
            .output()
            .map_err(|e| e.to_string())?;

        let mut text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !err.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&err);
        }
        if text.is_empty() {
            text = "(no output)".to_string();
        }

        Ok(format!("$ {command}\n{text}"))
    }
}

fn collect_tool_calls(message: &Message) -> Vec<(String, String, Vec<(String, String)>)> {
    message
        .content
        .iter()
        .filter_map(|c| match c {
            ContentItem::ToolCall {
                id,
                name,
                arguments,
            } => Some((id.clone(), name.clone(), arguments.clone())),
            _ => None,
        })
        .collect::<Vec<_>>()
}

fn signature_for_plan(tool_calls: &[(String, String, Vec<(String, String)>)]) -> String {
    tool_calls
        .iter()
        .map(|(_, name, args)| format!("{}:{:?}", name, args))
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::llm::{ContentItem, Message, ModelProvider, ToolSpec};
    use crate::tools::{AgentTool, AgentToolResult, ToolRegistry};

    struct RepeatingToolProvider;

    impl ModelProvider for RepeatingToolProvider {
        fn generate(&mut self, _messages: &[Message], _tools: &[ToolSpec]) -> Message {
            Message {
                role: "assistant".to_string(),
                content: vec![ContentItem::ToolCall {
                    id: "call_1".to_string(),
                    name: "noop".to_string(),
                    arguments: vec![("x".to_string(), "1".to_string())],
                }],
            }
        }
    }

    fn unique_tape_path() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("simple-agent-test-{nanos}.tape"))
    }

    #[test]
    fn stops_when_tool_plan_repeats() {
        let tape_path = unique_tape_path();
        let tape = TapeStore::new(tape_path.clone()).expect("tape init");

        let tool = AgentTool {
            spec: ToolSpec {
                name: "noop".to_string(),
                description: "no-op".to_string(),
                args: vec![],
            },
            exec: Box::new(|_, _| {
                Ok(AgentToolResult {
                    llm_output: "ok".to_string(),
                    ui_details: vec![],
                })
            }),
        };
        let registry = ToolRegistry::new(vec![tool]);

        let mut agent = AgentLoop::new(
            RepeatingToolProvider,
            registry,
            tape,
            std::env::current_dir().expect("cwd"),
        );

        let output = agent.handle_input("run noop").expect("handle input");
        assert!(output.contains("repeated the same tool plan"));

        let _ = fs::remove_file(tape_path);
    }
}
