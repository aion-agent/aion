use std::fs;
use std::path::{Path, PathBuf};
use clap::{arg, command, value_parser};

mod integrations;
use integrations::{load_all, build_system_prompt};

mod agent_direct;
use agent_direct::Agent;

mod memory;
use memory::Memory;

mod config;
use config::load_config;

mod tool_call;

fn ensure_workspace() -> anyhow::Result<()> {
    let dir = std::env::current_dir()?;
    if fs::read_dir(&dir)?.next().is_none() {
        fs::create_dir_all("integrations")?;
        fs::create_dir_all("memory")?;
        fs::create_dir_all("tools")?;
        fs::write("root.toml", r#"[agent]
name = "aion"
model = "hermes-3"
memory = "./memory/root.db"
temperature = 0.6
max_tokens = 1024
base_url = "http://localhost:8080"

[integrations]
discover = "./integrations/"
"#)?;
        println!("initialized new aion workspace");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ensure_workspace()?;

    let matches = command!()
        .arg(arg!([model_name] "The model name"))
        .arg(
            arg!(-c --config <FILE> "The configuration file")
                .value_parser(value_parser!(PathBuf)),
        )
        .arg(
            arg!(-i --integrations <DIRECTORY> "The integrations directory")
                .value_parser(value_parser!(PathBuf)),
        )
        .get_matches();

    let config_path = matches.get_one::<PathBuf>("config")
        .cloned()
        .unwrap_or_else(|| PathBuf::from("root.toml"));

    let config = load_config(&config_path)?;

    let model = matches.get_one::<String>("model_name")
        .cloned()
        .unwrap_or_else(|| config.agent.model.clone());

    let mut agent_config = config.agent.clone();
    agent_config.model = model;

    let integrations_dir = matches.get_one::<PathBuf>("integrations")
        .cloned()
        .unwrap_or_else(|| PathBuf::from(&config.integrations.discover));

    let memory = Memory::open(Path::new(&agent_config.memory))?;
    let integrations = load_all(&integrations_dir);
    let system_prompt = build_system_prompt(&integrations);

    let mut agent = Agent::new(
        agent_config,
        system_prompt,
        memory,
        integrations,
    );

    let mut input = String::new();
    loop {
        input.clear();
        std::io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        if trimmed == "exit" { break; }
        let reply = agent.step(trimmed).await?;
        println!("aion: {reply}");
    }

    Ok(())
}
