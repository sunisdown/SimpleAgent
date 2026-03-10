use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;

use crate::llm::{extract_text, ContentItem, Message, ModelProvider};
use crate::memory::TapeStore;
use crate::router::{RouteKind, Router};
use crate::tool_view::ProgressiveToolView;
use crate::tools::{make_tool_result, ToolRegistry};

pub struct AgentLoop<P: ModelProvider> {
    provider: P,
    tools: ToolRegistry,
    tape: TapeStore,
    workspace: PathBuf,
    router: Router,
    max_rounds: usize,
    context_window: usize,
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
        let mut seen_calls: HashSet<String> = HashSet::new();

        for _ in 0..self.max_rounds {
            self.tool_view.activate_hints(text);
            let assistant = self.provider.generate(&messages, &self.tool_view.specs());
            self.tape.append_message(&assistant)?;
            messages.push(assistant.clone());

            let tool_calls = assistant
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
                .collect::<Vec<_>>();

            if tool_calls.is_empty() {
                return Ok(extract_text(&assistant));
            }

            for (call_id, tool_name, args) in tool_calls {
                let signature = format!("{}:{:?}", tool_name, args);
                let tool_result = if seen_calls.contains(&signature) {
                    make_tool_result(
                        &call_id,
                        &tool_name,
                        format!("Skipped repeated tool call: {}", tool_name),
                    )
                } else {
                    seen_calls.insert(signature);
                    self.tool_view.note_selected(&tool_name);
                    match self.tools.execute(&self.workspace, &tool_name, &args) {
                        Ok(result) => make_tool_result(&call_id, &tool_name, result.text),
                        Err(err) => make_tool_result(&call_id, &tool_name, err),
                    }
                };

                self.tape.append_message(&tool_result)?;
                self.tape
                    .append_event("tool_call", &format!("tool={} args={:?}", tool_name, args))?;
                messages.push(tool_result);
            }
        }

        Ok("Tool-calling loop reached max rounds (15).".to_string())
    }

    fn handle_command(&mut self, command: &str, args: &str) -> Result<String, String> {
        match command {
            "help" | "h" => Ok(
                "Commands:\n,help\n,tools\n,tape.search <query>\n,handoff [name]\n,<shell command>"
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
            _ => self.run_shell(&format!("{} {}", command, args).trim().to_string()),
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
