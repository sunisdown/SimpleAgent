use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use crate::llm::{deserialize_context, serialize_context, Message};

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
        if !Path::new(&path).exists() {
            fs::write(&path, "").map_err(|e| e.to_string())?;
        }
        Ok(Self { path })
    }

    pub fn append_message(&self, message: &Message) -> Result<(), String> {
        self.append("message", &serialize_context(std::slice::from_ref(message)))
    }

    pub fn append_event(&self, event: &str, fields: &[(String, String)]) -> Result<(), String> {
        let flat = fields
            .iter()
            .map(|(k, v)| format!("{k}={}", escape(v)))
            .collect::<Vec<_>>()
            .join(";");
        self.append("event", &format!("event={event};{flat}"))
    }

    pub fn entries(&self) -> Result<Vec<TapeEntry>, String> {
        let file = OpenOptions::new()
            .read(true)
            .open(&self.path)
            .map_err(|e| e.to_string())?;

        let mut out = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line.map_err(|e| e.to_string())?;
            if line.trim().is_empty() {
                continue;
            }
            let mut parts = line.splitn(3, '\t');
            let id = parts
                .next()
                .unwrap_or("0")
                .parse::<usize>()
                .map_err(|e| e.to_string())?;
            let entry_type = parts.next().unwrap_or_default().to_string();
            let payload = parts.next().unwrap_or_default().to_string();
            out.push(TapeEntry {
                id,
                entry_type,
                payload,
            });
        }
        Ok(out)
    }

    pub fn build_messages(&self, context_window: usize) -> Result<Vec<Message>, String> {
        let mut msgs = Vec::new();
        for entry in self.entries()? {
            if entry.entry_type == "message" {
                msgs.extend(deserialize_context(&entry.payload));
            }
        }
        if msgs.len() <= context_window {
            Ok(msgs)
        } else {
            Ok(msgs[msgs.len() - context_window..].to_vec())
        }
    }

    fn append(&self, entry_type: &str, payload: &str) -> Result<(), String> {
        let next_id = self.entries()?.len() + 1;
        let mut file = OpenOptions::new()
            .append(true)
            .open(&self.path)
            .map_err(|e| e.to_string())?;
        writeln!(file, "{}\t{}\t{}", next_id, entry_type, payload).map_err(|e| e.to_string())
    }
}

fn escape(value: &str) -> String {
    value.replace(';', "\\s").replace('=', "\\e")
}
