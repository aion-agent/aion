use clap::{arg, command, value_parser};
use std::fs;
use std::path::{Path, PathBuf};

mod integrations;
use integrations::{build_system_prompt, load_all};

mod agent_direct;
use agent_direct::Agent;

mod memory;
use memory::Memory;

mod config;
use config::load_config;

mod tool_call;
mod tui;

fn ensure_workspace() -> anyhow::Result<()> {
    // Migration: rename existing .integration files to .toml
    if Path::new("integrations").exists() {
        if let Ok(entries) = fs::read_dir("integrations") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path
                    .extension()
                    .map(|x| x == "integration")
                    .unwrap_or(false)
                {
                    let mut new_path = path.clone();
                    new_path.set_extension("toml");
                    let _ = fs::rename(&path, &new_path);
                }
            }
        }
    }

    if !Path::new("root.toml").exists() {
        fs::create_dir_all("integrations")?;
        fs::create_dir_all("memory")?;
        fs::create_dir_all("tools")?;
        fs::write(
            "root.toml",
            r#"[agent]
name = "aion"
model = "hermes-3"
memory = "./memory/root.db"
temperature = 0.6
max_tokens = 1024
base_url = "http://localhost:8080"
max_history_messages = 20
max_tool_errors = 3
system_prompt = "system_prompt.txt"
# openrouter_model = "meta-llama/llama-3.1-8b-instruct:free"
# openrouter_api_key = "your-openrouter-api-key"

[integrations]
discover = "./integrations/"
"#,
        )?;

        fs::write(
            "system_prompt.txt",
            r#"You are Aion, a local AI agent with persistent memory and tool access.

<tools>
{{TOOLS}}
</tools>

When you want to call a tool respond with:
<tool_call>
{"name": "tool_name", "arguments": {...}}
</tool_call>

Think step by step. Use your child-databases to organize complex thoughts.
Wait for tool results before continuing."#,
        )?;

        // Seed default FS integration
        fs::write(
            "integrations/fs.toml",
            r#"[integration]
name = "fs"
description = "File System interactions"
tools = "./integrations/fs_tools.json"
executor = "deno run --allow-read --allow-write ./integrations/fs.js"
enabled = true"#,
        )?;

        fs::write(
            "integrations/fs_tools.json",
            r#"[
  {
    "name": "file_system",
    "description": "Read or write a file to the local file system.",
    "parameters": {
      "type": "object",
      "properties": {
        "action": { "type": "string", "enum": ["read", "write"] },
        "path": { "type": "string" },
        "content": { "type": "string", "description": "Content to write (only required for write action)" }
      },
      "required": ["action", "path"]
    }
  }
]"#,
        )?;

        fs::write(
            "integrations/fs.js",
            r#"const input = await new Response(Deno.stdin.readable).text();
const { action, path, content } = JSON.parse(input);

try {
    if (action === "read") {
        const text = await Deno.readTextFile(path);
        console.log(JSON.stringify({ status: "success", content: text }));
    } else if (action === "write") {
        await Deno.writeTextFile(path, content);
        console.log(JSON.stringify({ status: "success", message: `Wrote to ${path}` }));
    } else {
        console.log(JSON.stringify({ error: "Invalid action. Use 'read' or 'write'." }));
    }
} catch (e) {
    console.log(JSON.stringify({ error: e.message }));
}"#,
        )?;

        // Seed default Shell integration
        fs::write(
            "integrations/shell.toml",
            r#"[integration]
name = "shell"
description = "Execute shell commands"
tools = "./integrations/shell_tools.json"
executor = "deno run --allow-run ./integrations/shell.js"
enabled = true"#,
        )?;

        fs::write(
            "integrations/shell_tools.json",
            r#"[
  {
    "name": "execute_shell",
    "description": "Execute a safe bash shell command.",
    "parameters": {
      "type": "object",
      "properties": {
        "command": { "type": "string" }
      },
      "required": ["command"]
    }
  }
]"#,
        )?;

        fs::write(
            "integrations/shell.js",
            r#"const input = await new Response(Deno.stdin.readable).text();
const args = JSON.parse(input);
const { command } = args;

try {
    const p = new Deno.Command("sh", {
        args: ["-c", command],
        stdout: "piped",
        stderr: "piped"
    });
    const { code, stdout, stderr } = await p.output();
    console.log(JSON.stringify({
        status: code === 0 ? "success" : "error",
        exit_code: code,
        stdout: new TextDecoder().decode(stdout).trim(),
        stderr: new TextDecoder().decode(stderr).trim()
    }));
} catch (e) {
    console.log(JSON.stringify({ error: e.message }));
}"#,
        )?;

        println!("initialized new aion workspace");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    ensure_workspace()?;

    let matches = command!()
        .arg(arg!(-m --model <MODEL> "The model name").required(false))
        .arg(
            clap::Arg::new("openrouter")
                .long("openrouter")
                .short('o')
                .alias("ol")
                .action(clap::ArgAction::SetTrue)
                .help("Use OpenRouter API"),
        )
        .arg(arg!(-c --config <FILE> "The configuration file").value_parser(value_parser!(PathBuf)))
        .arg(
            arg!(-i --integrations <DIRECTORY> "The integrations directory")
                .value_parser(value_parser!(PathBuf)),
        )
        .get_matches();

    let config_path = matches
        .get_one::<PathBuf>("config")
        .cloned()
        .unwrap_or_else(|| PathBuf::from("root.toml"));

    let config = load_config(&config_path)?;

    let use_openrouter = matches.get_flag("openrouter");

    let model_opt = matches.get_one::<String>("model").cloned();
    let model = if use_openrouter {
        model_opt
            .or_else(|| config.agent.openrouter_model.clone())
            .ok_or_else(|| {
                anyhow::anyhow!("No OpenRouter model specified in config or command line via -m")
            })?
    } else {
        model_opt.unwrap_or_else(|| config.agent.model.clone())
    };

    let mut agent_config = config.agent.clone();
    agent_config.model = model;

    // Print agent name to show usage and prevent unused warnings
    println!("Loading configurations for Agent: {}", agent_config.name);

    let integrations_dir = matches
        .get_one::<PathBuf>("integrations")
        .cloned()
        .unwrap_or_else(|| PathBuf::from(&config.integrations.discover));

    let memory = Memory::open(Path::new(&agent_config.memory))?;
    let integrations = load_all(&integrations_dir);
    let system_prompt = build_system_prompt(Path::new(&agent_config.system_prompt), &integrations);

    let agent = Agent::new(
        agent_config,
        system_prompt,
        memory,
        integrations,
        use_openrouter,
    );

    tui::run_tui(agent).await?;

    Ok(())
}
