use std::collections::HashSet;

use crate::llm::ToolSpec;

pub struct ProgressiveToolView {
    all: Vec<ToolSpec>,
    active: HashSet<String>,
}

impl ProgressiveToolView {
    pub fn new(all: Vec<ToolSpec>) -> Self {
        let mut active = HashSet::new();
        for tool in &all {
            if tool.name == "read" {
                active.insert(tool.name.clone());
            }
        }
        Self { all, active }
    }

    pub fn activate_hints(&mut self, input: &str) {
        let input = input.to_lowercase();
        if input.contains("bash") || input.contains("command") {
            self.active.insert("bash".to_string());
        }
        if input.contains("write") || input.contains("edit") {
            self.active.insert("write".to_string());
            self.active.insert("edit".to_string());
        }
        if input.contains("read") || input.contains("file") {
            self.active.insert("read".to_string());
        }
    }

    pub fn note_selected(&mut self, tool: &str) {
        self.active.insert(tool.to_string());
    }

    pub fn specs(&self) -> Vec<ToolSpec> {
        self.all
            .iter()
            .filter(|tool| self.active.contains(&tool.name))
            .cloned()
            .collect()
    }
}
