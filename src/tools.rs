use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::llm::{ContentItem, Message, ToolSpec};

pub struct AgentToolResult {
    pub text: String,
}

type ToolExec = dyn Fn(&Path, &[(String, String)]) -> Result<AgentToolResult, String> + Send + Sync;

pub struct AgentTool {
    pub spec: ToolSpec,
    pub exec: Box<ToolExec>,
}

pub struct ToolRegistry {
    tools: HashMap<String, AgentTool>,
}

impl ToolRegistry {
    pub fn new(tools: Vec<AgentTool>) -> Self {
        let map = tools
            .into_iter()
            .map(|t| (t.spec.name.clone(), t))
            .collect::<HashMap<_, _>>();
        Self { tools: map }
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|t| t.spec.clone()).collect()
    }

    pub fn execute(
        &self,
        cwd: &Path,
        name: &str,
        args: &[(String, String)],
    ) -> Result<AgentToolResult, String> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| format!("Tool {name} not found"))?;
        (tool.exec)(cwd, args)
    }
}

pub fn create_default_tools() -> Vec<AgentTool> {
    vec![create_ls_tool(), create_read_tool(), create_bash_tool()]
}

pub fn make_tool_result(_call_id: &str, _tool_name: &str, text: String) -> Message {
    Message {
        role: "toolResult".to_string(),
        content: vec![ContentItem::Text(text)],
    }
}

fn create_ls_tool() -> AgentTool {
    AgentTool {
        spec: ToolSpec {
            name: "ls".to_string(),
            description: "List directory contents".to_string(),
        },
        exec: Box::new(|cwd, args| {
            let path = arg(args, "path").unwrap_or_else(|| ".".to_string());
            let resolved = resolve(cwd, &path);
            if !resolved.exists() {
                return Err(format!("Path not found: {}", resolved.display()));
            }
            if !resolved.is_dir() {
                return Err(format!("Not a directory: {}", resolved.display()));
            }
            let mut names = fs::read_dir(resolved)
                .map_err(|e| e.to_string())?
                .flatten()
                .map(|e| {
                    let mut name = e.file_name().to_string_lossy().to_string();
                    if e.path().is_dir() {
                        name.push('/');
                    }
                    name
                })
                .collect::<Vec<_>>();
            names.sort();
            Ok(AgentToolResult {
                text: if names.is_empty() {
                    "(empty directory)".to_string()
                } else {
                    names.join("\n")
                },
            })
        }),
    }
}

fn create_read_tool() -> AgentTool {
    AgentTool {
        spec: ToolSpec {
            name: "read".to_string(),
            description: "Read file content".to_string(),
        },
        exec: Box::new(|cwd, args| {
            let path =
                arg(args, "path").ok_or_else(|| "Missing required argument: path".to_string())?;
            let resolved = resolve(cwd, &path);
            if !resolved.exists() {
                return Err(format!("Path not found: {}", resolved.display()));
            }
            if !resolved.is_file() {
                return Err(format!("Not a file: {}", resolved.display()));
            }
            let text = fs::read_to_string(resolved).map_err(|e| e.to_string())?;
            Ok(AgentToolResult { text })
        }),
    }
}

fn create_bash_tool() -> AgentTool {
    AgentTool {
        spec: ToolSpec {
            name: "bash".to_string(),
            description: "Execute shell command in workspace".to_string(),
        },
        exec: Box::new(|cwd, args| {
            let command = arg(args, "command")
                .ok_or_else(|| "Missing required argument: command".to_string())?;
            let out = Command::new("/usr/bin/timeout")
                .arg("30")
                .arg("/bin/sh")
                .arg("-c")
                .arg(command)
                .current_dir(cwd)
                .output()
                .map_err(|e| e.to_string())?;
            let mut text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
            if !stderr.is_empty() {
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&stderr);
            }
            if text.is_empty() {
                text = "(no output)".to_string();
            }
            Ok(AgentToolResult { text })
        }),
    }
}

fn resolve(cwd: &Path, file: &str) -> PathBuf {
    let p = PathBuf::from(file);
    if p.is_absolute() {
        p
    } else {
        cwd.join(p)
    }
}

fn arg(args: &[(String, String)], key: &str) -> Option<String> {
    args.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
}
