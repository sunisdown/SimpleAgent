use std::collections::{HashMap, HashSet};

use crate::llm::ToolSpec;

pub struct ProgressiveToolView {
    tools: HashMap<String, ToolSpec>,
    active: HashSet<String>,
}

impl ProgressiveToolView {
    pub fn new(tools: Vec<ToolSpec>) -> Self {
        Self {
            tools: tools.into_iter().map(|t| (t.name.clone(), t)).collect(),
            active: HashSet::new(),
        }
    }

    pub fn activate_hints(&mut self, text: &str) {
        for word in text.split_whitespace() {
            if let Some(name) = word.strip_prefix('$') {
                if self.tools.contains_key(name) {
                    self.active.insert(name.to_string());
                }
            }
        }
    }

    pub fn note_selected(&mut self, tool_name: &str) {
        if self.tools.contains_key(tool_name) {
            self.active.insert(tool_name.to_string());
        }
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        if self.active.is_empty() {
            return self.tools.values().cloned().collect();
        }
        self.active
            .iter()
            .filter_map(|name| self.tools.get(name).cloned())
            .collect()
    }
}
