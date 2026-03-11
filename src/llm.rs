#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolArgSpec {
    pub name: String,
    pub required: bool,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub args: Vec<ToolArgSpec>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentItem {
    Text(String),
    ToolCall {
        id: String,
        name: String,
        arguments: Vec<(String, String)>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub role: String,
    pub content: Vec<ContentItem>,
}

#[derive(Clone, Debug)]
pub struct ProviderRequest {
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSpec>,
}

#[derive(Clone, Debug, Default)]
pub struct ProviderUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub total_tokens: usize,
}

#[derive(Clone, Debug)]
pub enum StreamEvent {
    TextDelta(String),
    ToolCallDelta { name: String, partial_args: String },
    Done(ProviderUsage),
}

#[derive(Clone, Debug)]
pub struct ProviderResponse {
    pub message: Message,
    pub usage: ProviderUsage,
    pub stop_reason: String,
    pub provider_id: String,
}

pub trait ModelProvider {
    fn generate(&mut self, request: ProviderRequest) -> ProviderResponse;
    fn stream_generate(&mut self, request: ProviderRequest) -> Vec<StreamEvent>;
}

pub struct MockProvider {
    call_counter: usize,
}

impl MockProvider {
    pub fn new() -> Self {
        Self { call_counter: 0 }
    }

    fn tool_call(&mut self, text: &str, name: &str, args: Vec<(String, String)>) -> Message {
        self.call_counter += 1;
        Message {
            role: "assistant".to_string(),
            content: vec![
                ContentItem::Text(text.to_string()),
                ContentItem::ToolCall {
                    id: format!("call_{}", self.call_counter),
                    name: name.to_string(),
                    arguments: args,
                },
            ],
        }
    }

    fn decide(&mut self, request: &ProviderRequest) -> Message {
        let latest = request.messages.last();
        let latest_text = latest.map(extract_text).unwrap_or_default().to_lowercase();
        let latest_role = latest.map(|m| m.role.as_str()).unwrap_or("user");

        if latest_role == "tool" {
            return Message {
                role: "assistant".to_string(),
                content: vec![ContentItem::Text(format!(
                    "Done.\n{}",
                    latest.map(extract_text).unwrap_or_default()
                ))],
            };
        }

        if latest_text.starts_with("read ") && has_tool(&request.tools, "read") {
            return self.tool_call(
                "Reading file.",
                "read",
                vec![("path".to_string(), latest_text.replacen("read ", "", 1))],
            );
        }
        if latest_text.starts_with("bash ") && has_tool(&request.tools, "bash") {
            return self.tool_call(
                "Running command.",
                "bash",
                vec![("command".to_string(), latest_text.replacen("bash ", "", 1))],
            );
        }

        Message {
            role: "assistant".to_string(),
            content: vec![ContentItem::Text(
                "Mock provider: try 'read <path>' or 'bash <command>'.".to_string(),
            )],
        }
    }
}

impl ModelProvider for MockProvider {
    fn generate(&mut self, request: ProviderRequest) -> ProviderResponse {
        let msg = self.decide(&request);
        let input_tokens = request
            .messages
            .iter()
            .map(extract_text)
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .count();
        let output_tokens = extract_text(&msg).split_whitespace().count();
        let usage = ProviderUsage {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
        };
        ProviderResponse {
            message: msg,
            usage,
            stop_reason: "completed".to_string(),
            provider_id: "mock-openai".to_string(),
        }
    }

    fn stream_generate(&mut self, request: ProviderRequest) -> Vec<StreamEvent> {
        let response = self.generate(request);
        let mut events = Vec::new();
        for item in &response.message.content {
            match item {
                ContentItem::Text(t) => events.push(StreamEvent::TextDelta(t.clone())),
                ContentItem::ToolCall {
                    name, arguments, ..
                } => events.push(StreamEvent::ToolCallDelta {
                    name: name.clone(),
                    partial_args: arguments
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(","),
                }),
            }
        }
        events.push(StreamEvent::Done(response.usage));
        events
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

pub fn serialize_context(messages: &[Message]) -> String {
    messages
        .iter()
        .map(|m| {
            let text = extract_text(m).replace('\n', "\\n").replace('|', "\\p");
            format!("{}|{}", m.role, text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn deserialize_context(serialized: &str) -> Vec<Message> {
    serialized
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(2, '|');
            let role = parts.next()?.to_string();
            let text = parts
                .next()
                .unwrap_or_default()
                .replace("\\n", "\n")
                .replace("\\p", "|");
            Some(Message {
                role,
                content: vec![ContentItem::Text(text)],
            })
        })
        .collect()
}

fn has_tool(tools: &[ToolSpec], name: &str) -> bool {
    tools.iter().any(|t| t.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_roundtrip() {
        let input = vec![Message {
            role: "user".to_string(),
            content: vec![ContentItem::Text("a|b\nline2".to_string())],
        }];
        assert_eq!(deserialize_context(&serialize_context(&input)), input);
    }
}
