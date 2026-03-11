use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

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
    pub max_output_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub stream: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ProviderUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub total_tokens: usize,
    pub estimated_cost_usd: f64,
}

#[derive(Clone, Debug)]
pub struct ProviderResponse {
    pub message: Message,
    pub usage: ProviderUsage,
    pub stop_reason: String,
    pub provider_id: String,
    pub normalization_notes: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum StreamEvent {
    TextDelta(String),
    ToolCallDelta { name: String, partial_args: String },
    Done(ProviderUsage),
}

#[derive(Clone, Debug, Default)]
pub struct AbortSignal {
    inner: Arc<AtomicBool>,
}

impl AbortSignal {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.inner.store(true, Ordering::SeqCst);
    }

    pub fn cancelled(&self) -> bool {
        self.inner.load(Ordering::SeqCst)
    }
}

pub trait ModelProvider {
    fn generate(&mut self, request: ProviderRequest, abort: &AbortSignal) -> ProviderResponse;
    fn stream_generate(
        &mut self,
        request: ProviderRequest,
        abort: &AbortSignal,
    ) -> Result<Vec<StreamEvent>, String>;
}

#[derive(Clone, Copy, Debug)]
pub enum ProviderAdapterKind {
    OpenAiLike,
    AnthropicLike,
}

#[derive(Clone, Debug)]
struct ProviderQuirks {
    supports_temperature: bool,
    token_field_name: &'static str,
}

pub struct MockProvider {
    counter: usize,
    adapter: ProviderAdapterKind,
}

impl MockProvider {
    pub fn new() -> Self {
        Self {
            counter: 0,
            adapter: ProviderAdapterKind::OpenAiLike,
        }
    }

    pub fn with_adapter(adapter: ProviderAdapterKind) -> Self {
        Self {
            counter: 0,
            adapter,
        }
    }

    fn quirks(&self) -> ProviderQuirks {
        match self.adapter {
            ProviderAdapterKind::OpenAiLike => ProviderQuirks {
                supports_temperature: true,
                token_field_name: "max_output_tokens",
            },
            ProviderAdapterKind::AnthropicLike => ProviderQuirks {
                supports_temperature: false,
                token_field_name: "max_tokens",
            },
        }
    }

    fn provider_id(&self) -> String {
        match self.adapter {
            ProviderAdapterKind::OpenAiLike => "mock-openai".to_string(),
            ProviderAdapterKind::AnthropicLike => "mock-anthropic".to_string(),
        }
    }

    fn normalize_request(&self, mut request: ProviderRequest) -> (ProviderRequest, Vec<String>) {
        let quirks = self.quirks();
        let mut notes = vec![format!(
            "normalized_token_field={}",
            quirks.token_field_name
        )];

        if request.max_output_tokens.is_none() {
            request.max_output_tokens = Some(512);
            notes.push("defaulted_max_output_tokens=512".to_string());
        }

        if !quirks.supports_temperature && request.temperature.is_some() {
            request.temperature = None;
            notes.push("temperature_dropped_for_provider".to_string());
        }

        (request, notes)
    }

    fn estimate_usage(&self, input: &[Message], output: &Message) -> ProviderUsage {
        let input_text = input.iter().map(extract_text).collect::<Vec<_>>().join(" ");
        let output_text = extract_text(output);
        let input_tokens = rough_token_estimate(&input_text);
        let output_tokens = rough_token_estimate(&output_text);
        let total_tokens = input_tokens + output_tokens;

        ProviderUsage {
            input_tokens,
            output_tokens,
            total_tokens,
            estimated_cost_usd: (total_tokens as f64) * 0.000_001,
        }
    }

    fn decide_message(&mut self, messages: &[Message], tools: &[ToolSpec]) -> Message {
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

impl ModelProvider for MockProvider {
    fn generate(&mut self, request: ProviderRequest, abort: &AbortSignal) -> ProviderResponse {
        let (normalized, notes) = self.normalize_request(request);
        if abort.cancelled() {
            return ProviderResponse {
                message: Message {
                    role: "assistant".to_string(),
                    content: vec![ContentItem::Text("Generation cancelled.".to_string())],
                },
                usage: ProviderUsage::default(),
                stop_reason: "cancelled".to_string(),
                provider_id: self.provider_id(),
                normalization_notes: notes,
            };
        }

        let message = self.decide_message(&normalized.messages, &normalized.tools);
        let usage = self.estimate_usage(&normalized.messages, &message);
        ProviderResponse {
            message,
            usage,
            stop_reason: "completed".to_string(),
            provider_id: self.provider_id(),
            normalization_notes: notes,
        }
    }

    fn stream_generate(
        &mut self,
        request: ProviderRequest,
        abort: &AbortSignal,
    ) -> Result<Vec<StreamEvent>, String> {
        let response = self.generate(request, abort);
        if response.stop_reason == "cancelled" {
            return Ok(vec![StreamEvent::Done(ProviderUsage::default())]);
        }

        let mut events = Vec::new();
        for item in &response.message.content {
            match item {
                ContentItem::Text(t) => events.push(StreamEvent::TextDelta(t.clone())),
                ContentItem::ToolCall {
                    name, arguments, ..
                } => {
                    let partial = arguments
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(",");
                    events.push(StreamEvent::ToolCallDelta {
                        name: name.clone(),
                        partial_args: partial,
                    });
                }
            }
        }
        events.push(StreamEvent::Done(response.usage));
        Ok(events)
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
            let text = extract_text(m)
                .replace('\\', "\\\\")
                .replace('\n', "\\n")
                .replace('|', "\\|");
            format!("{}|{}", m.role, text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn deserialize_context(serialized: &str) -> Vec<Message> {
    serialized
        .lines()
        .filter_map(|line| {
            let mut split = line.splitn(2, '|');
            let role = split.next()?.to_string();
            let raw = split.next().unwrap_or_default();
            let text = unescape_context(raw);
            Some(Message {
                role,
                content: vec![ContentItem::Text(text)],
            })
        })
        .collect::<Vec<_>>()
}

fn rough_token_estimate(text: &str) -> usize {
    text.split_whitespace().count().max(1)
}

fn unescape_context(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'n' => out.push('\n'),
                    '|' => out.push('|'),
                    '\\' => out.push('\\'),
                    _ => {
                        out.push('\\');
                        out.push(next);
                    }
                }
            } else {
                out.push('\\');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn has_tool(tools: &[ToolSpec], name: &str) -> bool {
    tools.iter().any(|t| t.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anthropic_adapter_drops_temperature() {
        let mut provider = MockProvider::with_adapter(ProviderAdapterKind::AnthropicLike);
        let response = provider.generate(
            ProviderRequest {
                messages: vec![Message {
                    role: "user".to_string(),
                    content: vec![ContentItem::Text("hello".to_string())],
                }],
                tools: vec![],
                max_output_tokens: Some(32),
                temperature: Some(0.2),
                stream: false,
            },
            &AbortSignal::new(),
        );

        assert!(response
            .normalization_notes
            .iter()
            .any(|n| n == "temperature_dropped_for_provider"));
    }

    #[test]
    fn context_fixture_roundtrip() {
        let fixture = include_str!("../fixtures/context/session-v1.ctx");
        let messages = deserialize_context(fixture);
        assert_eq!(messages.len(), 3);

        let serialized = serialize_context(&messages);
        let reparsed = deserialize_context(&serialized);
        assert_eq!(messages, reparsed);
    }
}
