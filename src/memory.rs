use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use crate::llm::{ContentItem, Message};

#[derive(Clone, Debug)]
pub struct TapeEntry {
    pub id: usize,
    pub entry_type: String,
    pub payload: String,
}

pub struct TapeStore {
    path: PathBuf,
}

impl TapeStore {
    pub fn new(path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if !path.exists() {
            fs::File::create(&path).map_err(|e| e.to_string())?;
        }
        Ok(Self { path })
    }

    pub fn append_message(&self, message: &Message) -> Result<TapeEntry, String> {
        let text = message
            .content
            .iter()
            .filter_map(|c| match c {
                ContentItem::Text(t) => Some(t.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\\n");
        self.append("message", &format!("{}|{}", message.role, sanitize(&text)))
    }

    pub fn append_event(&self, event: &str, payload: &str) -> Result<TapeEntry, String> {
        self.append("event", &format!("{}|{}", event, sanitize(payload)))
    }

    pub fn append_anchor(
        &self,
        name: &str,
        payload: &str,
        entry_type: &str,
    ) -> Result<TapeEntry, String> {
        self.append(entry_type, &format!("{}|{}", name, sanitize(payload)))
    }

    pub fn entries(&self) -> Result<Vec<TapeEntry>, String> {
        let text = fs::read_to_string(&self.path).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for line in text.lines() {
            let parts = line.splitn(3, '\t').collect::<Vec<_>>();
            if parts.len() != 3 {
                continue;
            }
            let id = parts[0].parse::<usize>().unwrap_or(0);
            out.push(TapeEntry {
                id,
                entry_type: parts[1].to_string(),
                payload: parts[2].to_string(),
            });
        }
        Ok(out)
    }

    pub fn search(&self, query: &str) -> Result<Vec<TapeEntry>, String> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return Ok(vec![]);
        }
        Ok(self
            .entries()?
            .into_iter()
            .filter(|e| {
                format!("{} {}", e.entry_type, e.payload)
                    .to_lowercase()
                    .contains(&q)
            })
            .collect())
    }

    pub fn build_messages(&self, window: usize) -> Result<Vec<Message>, String> {
        let entries = self.entries()?;
        let start = entries
            .iter()
            .enumerate()
            .rev()
            .find(|(_, e)| e.entry_type == "handoff")
            .map(|(idx, _)| idx + 1)
            .unwrap_or(0);

        let mut messages = Vec::new();
        for e in entries.iter().skip(start) {
            if e.entry_type != "message" {
                continue;
            }
            let parts = e.payload.splitn(2, '|').collect::<Vec<_>>();
            if parts.len() != 2 {
                continue;
            }
            messages.push(Message {
                role: parts[0].to_string(),
                content: vec![ContentItem::Text(desanitize(parts[1]))],
            });
        }

        if messages.len() > window {
            Ok(messages[messages.len() - window..].to_vec())
        } else {
            Ok(messages)
        }
    }

    fn append(&self, entry_type: &str, payload: &str) -> Result<TapeEntry, String> {
        let next_id = self.entries()?.last().map(|e| e.id + 1).unwrap_or(1);
        let line = format!("{}\t{}\t{}\n", next_id, entry_type, payload);
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.path)
            .map_err(|e| e.to_string())?;
        file.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
        Ok(TapeEntry {
            id: next_id,
            entry_type: entry_type.to_string(),
            payload: payload.to_string(),
        })
    }
}

fn sanitize(s: &str) -> String {
    s.replace('\n', "\\n").replace('\t', "\\t")
}

fn desanitize(s: &str) -> String {
    s.replace("\\n", "\n").replace("\\t", "\t")
}
