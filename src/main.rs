mod core;
mod llm;
mod memory;
mod router;
mod runtime;
mod tool_view;
mod tools;

use std::env;
use std::path::PathBuf;

use crate::core::AgentLoop;
use crate::llm::MockProvider;
use crate::memory::TapeStore;
use crate::runtime::RuntimeProfile;

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        return Err("Usage: simple-agent [--cwd <path>] [--session <id>] [--profile <yolo|readonly|custom>] [--tools <a,b>] <prompt>".to_string());
    }

    let mut cwd = PathBuf::from(".");
    let mut session = "default".to_string();
    let mut profile_name = "yolo".to_string();
    let mut custom_tools: Option<String> = None;
    let mut prompt_parts = Vec::<String>::new();

    while !args.is_empty() {
        let arg = args.remove(0);
        if arg == "--cwd" && !args.is_empty() {
            cwd = PathBuf::from(args.remove(0));
            continue;
        }
        if arg == "--session" && !args.is_empty() {
            session = args.remove(0);
            continue;
        }
        if arg == "--profile" && !args.is_empty() {
            profile_name = args.remove(0);
            continue;
        }
        if arg == "--tools" && !args.is_empty() {
            custom_tools = Some(args.remove(0));
            continue;
        }
        prompt_parts.push(arg);
        prompt_parts.append(&mut args);
        break;
    }

    let prompt = prompt_parts.join(" ").trim().to_string();
    if prompt.is_empty() {
        return Err("prompt is required".to_string());
    }

    let profile = RuntimeProfile::parse(&profile_name, custom_tools.as_deref())?;

    let workspace = cwd.canonicalize().unwrap_or(cwd);
    let tape = TapeStore::new(
        workspace
            .join(".simple_agent")
            .join(format!("{session}.tape")),
    )?;
    let registry = profile.tool_registry()?;
    let mut loop_engine = AgentLoop::new(MockProvider::new(), registry, tape, workspace, profile);

    let output = loop_engine.handle_input(&prompt)?;
    println!("{output}");
    Ok(())
}
