#[derive(Clone, Debug)]
pub struct LoopLimits {
    pub max_rounds: usize,
    pub context_window: usize,
    pub max_tool_calls_per_round: usize,
}

impl Default for LoopLimits {
    fn default() -> Self {
        Self {
            max_rounds: 15,
            context_window: 50,
            max_tool_calls_per_round: 4,
        }
    }
}

#[derive(Clone, Debug)]
pub struct AgentConfig {
    pub prompt_version: &'static str,
    pub system_prompt: &'static str,
    pub limits: LoopLimits,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            prompt_version: "v1",
            system_prompt: "You are SimpleAgent. Be concise, deterministic, and observable. Use tools when needed. Avoid hidden assumptions.",
            limits: LoopLimits::default(),
        }
    }
}
