#[derive(Clone, Debug)]
pub struct ToolArgSpec {
    pub name: String,
    pub description: String,
    pub required: bool,
}

#[derive(Clone, Debug)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub args: Vec<ToolArgSpec>,
}

#[derive(Clone, Debug)]
pub enum ContentItem {
    Text(String),
    ToolCall {
        id: String,
        name: String,
        arguments: Vec<(String, String)>,
    },
}

#[derive(Clone, Debug)]
pub struct Message {
    pub role: String,
    pub content: Vec<ContentItem>,
}

pub trait ModelProvider {
    fn generate(&mut self, messages: &[Message], tools: &[ToolSpec]) -> Message;
}

pub struct MockProvider {
    counter: usize,
}

impl MockProvider {
    pub fn new() -> Self {
        Self { counter: 0 }
    }
}

impl ModelProvider for MockProvider {
    fn generate(&mut self, messages: &[Message], tools: &[ToolSpec]) -> Message {
        let latest = messages.last();
        let role = latest.map(|m| m.role.as_str()).unwrap_or("user");
        let text = latest.map(extract_text).unwrap_or_default().to_lowercase();

        if role == "toolResult" {
            return Message {
                role: "assistant".to_string(),
                content: vec![ContentItem::Text(format!(
                    "Tool result:\n{}",
                    latest.map(extract_text).unwrap_or_default()
                ))],
            };
        }

        if text.starts_with("read ") && has_tool(tools, "read") {
            let path = text.replacen("read ", "", 1);
            return self.tool_call(
                "I'll read that file.",
                "read",
                vec![("path".to_string(), path)],
            );
        }
        if text.contains("ls") && has_tool(tools, "ls") {
            return self.tool_call(
                "I'll list the directory.",
                "ls",
                vec![("path".to_string(), ".".to_string())],
            );
        }
        if text.starts_with("bash ") && has_tool(tools, "bash") {
            let cmd = text.replacen("bash ", "", 1);
            return self.tool_call(
                "I'll run that command.",
                "bash",
                vec![("command".to_string(), cmd)],
            );
        }

        let names = tools
            .iter()
            .map(|t| t.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        Message {
            role: "assistant".to_string(),
            content: vec![ContentItem::Text(format!(
                "Mock provider active. Try: 'ls', 'read <path>', or 'bash <command>'. Available tools: {names}."
            ))],
        }
    }
}

impl MockProvider {
    fn tool_call(&mut self, text: &str, name: &str, args: Vec<(String, String)>) -> Message {
        self.counter += 1;
        Message {
            role: "assistant".to_string(),
            content: vec![
                ContentItem::Text(text.to_string()),
                ContentItem::ToolCall {
                    id: format!("call_{}", self.counter),
                    name: name.to_string(),
                    arguments: args,
                },
            ],
        }
    }
}

pub fn extract_text(message: &Message) -> String {
    message
        .content
        .iter()
        .filter_map(|c| match c {
            ContentItem::Text(t) => Some(t.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn has_tool(tools: &[ToolSpec], name: &str) -> bool {
    tools.iter().any(|t| t.name == name)
}
