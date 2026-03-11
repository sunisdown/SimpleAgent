use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::llm::{ContentItem, Message, ToolArgSpec, ToolSpec};
use crate::runtime::RuntimeProfile;

pub struct AgentToolResult {
    pub llm_output: String,
    pub ui_details: Vec<(String, String)>,
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
        Self {
            tools: tools
                .into_iter()
                .map(|t| (t.spec.name.clone(), t))
                .collect(),
        }
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
            .ok_or_else(|| format!("Unknown tool: {name}"))?;
        validate_args(&tool.spec, args)?;
        (tool.exec)(cwd, args)
    }
}

pub fn create_profile_tools(profile: &RuntimeProfile) -> Result<Vec<AgentTool>, String> {
    let all = vec![
        create_read_tool(),
        create_write_tool(),
        create_edit_tool(),
        create_bash_tool(),
    ];
    match profile {
        RuntimeProfile::Yolo => Ok(all),
        RuntimeProfile::Readonly => Ok(vec![create_read_tool()]),
        RuntimeProfile::Custom(allowlist) => {
            let allowed = allowlist.iter().cloned().collect::<HashSet<_>>();
            let selected = all
                .into_iter()
                .filter(|t| allowed.contains(&t.spec.name))
                .collect::<Vec<_>>();
            if selected.is_empty() {
                return Err("No valid tools selected. Valid: read,write,edit,bash".to_string());
            }
            Ok(selected)
        }
    }
}

pub fn make_tool_message(result: &AgentToolResult) -> Message {
    Message {
        role: "tool".to_string(),
        content: vec![ContentItem::Text(result.llm_output.clone())],
    }
}

fn create_read_tool() -> AgentTool {
    AgentTool {
        spec: ToolSpec {
            name: "read".to_string(),
            description: "Read UTF-8 file content".to_string(),
            args: vec![ToolArgSpec {
                name: "path".to_string(),
                required: true,
                description: "Path to file".to_string(),
            }],
        },
        exec: Box::new(|cwd, args| {
            let path = arg_required(args, "path")?;
            let resolved = resolve(cwd, &path);
            let text = fs::read_to_string(&resolved).map_err(|e| e.to_string())?;
            Ok(AgentToolResult {
                llm_output: text,
                ui_details: vec![
                    ("tool".to_string(), "read".to_string()),
                    ("path".to_string(), resolved.display().to_string()),
                ],
            })
        }),
    }
}

fn create_write_tool() -> AgentTool {
    AgentTool {
        spec: ToolSpec {
            name: "write".to_string(),
            description: "Write full content to file".to_string(),
            args: vec![
                ToolArgSpec {
                    name: "path".to_string(),
                    required: true,
                    description: "Path to file".to_string(),
                },
                ToolArgSpec {
                    name: "content".to_string(),
                    required: true,
                    description: "New file content".to_string(),
                },
            ],
        },
        exec: Box::new(|cwd, args| {
            let path = arg_required(args, "path")?;
            let content = arg_required(args, "content")?;
            let resolved = resolve(cwd, &path);
            if let Some(parent) = resolved.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(&resolved, content.as_bytes()).map_err(|e| e.to_string())?;
            Ok(AgentToolResult {
                llm_output: format!("wrote {} bytes", content.len()),
                ui_details: vec![("path".to_string(), resolved.display().to_string())],
            })
        }),
    }
}

fn create_edit_tool() -> AgentTool {
    AgentTool {
        spec: ToolSpec {
            name: "edit".to_string(),
            description: "Replace substring in file".to_string(),
            args: vec![
                ToolArgSpec {
                    name: "path".to_string(),
                    required: true,
                    description: "Path to file".to_string(),
                },
                ToolArgSpec {
                    name: "find".to_string(),
                    required: true,
                    description: "Text to find".to_string(),
                },
                ToolArgSpec {
                    name: "replace".to_string(),
                    required: true,
                    description: "Replacement text".to_string(),
                },
            ],
        },
        exec: Box::new(|cwd, args| {
            let path = arg_required(args, "path")?;
            let find = arg_required(args, "find")?;
            let replace = arg_required(args, "replace")?;
            let resolved = resolve(cwd, &path);
            let text = fs::read_to_string(&resolved).map_err(|e| e.to_string())?;
            if !text.contains(&find) {
                return Err("edit failed: find text not found".to_string());
            }
            let updated = text.replace(&find, &replace);
            fs::write(&resolved, updated.as_bytes()).map_err(|e| e.to_string())?;
            Ok(AgentToolResult {
                llm_output: "edit complete".to_string(),
                ui_details: vec![("path".to_string(), resolved.display().to_string())],
            })
        }),
    }
}

fn create_bash_tool() -> AgentTool {
    AgentTool {
        spec: ToolSpec {
            name: "bash".to_string(),
            description: "Execute shell command".to_string(),
            args: vec![ToolArgSpec {
                name: "command".to_string(),
                required: true,
                description: "Command to run".to_string(),
            }],
        },
        exec: Box::new(|cwd, args| {
            let command = arg_required(args, "command")?;
            let output = Command::new("/usr/bin/timeout")
                .arg("30")
                .arg("/bin/sh")
                .arg("-c")
                .arg(&command)
                .current_dir(cwd)
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

            Ok(AgentToolResult {
                llm_output: if text.is_empty() {
                    "(no output)".to_string()
                } else {
                    text
                },
                ui_details: vec![("command".to_string(), command)],
            })
        }),
    }
}

fn arg_required(args: &[(String, String)], name: &str) -> Result<String, String> {
    args.iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
        .ok_or_else(|| format!("Missing required argument: {name}"))
}

fn validate_args(spec: &ToolSpec, args: &[(String, String)]) -> Result<(), String> {
    for a in &spec.args {
        if a.required && !args.iter().any(|(k, _)| k == &a.name) {
            return Err(format!("Missing required argument: {}", a.name));
        }
    }
    let allowed = spec
        .args
        .iter()
        .map(|a| a.name.clone())
        .collect::<HashSet<_>>();
    for (k, _) in args {
        if !allowed.contains(k) {
            return Err(format!(
                "Unknown argument '{}' for tool '{}'.",
                k, spec.name
            ));
        }
    }
    Ok(())
}

fn resolve(cwd: &Path, path: &str) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        p
    } else {
        cwd.join(p)
    }
}
