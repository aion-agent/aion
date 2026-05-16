# Aion Local AI Agent

Aion is a highly modular, local-first AI agent written in Rust. It is engineered specifically to interface with local OpenAI-compatible endpoints (like `llama.cpp` serving Hermes models) to orchestrate complex tool-calling and long-term memory management.

## Features

- **Hermes Tool Calling Protocol**: Aion features a custom ChatML parser that streams LLM responses to your terminal while silently buffering and executing `<tool_call>` XML blocks in the background.
- **Hierarchical Memory**: Built on top of `rusqlite` and `fts5`, Aion manages a long-term `root.db` memory store and can autonomously spawn isolated "child scopes" for compartmentalized reasoning.
- **Dynamic Deno Integrations**: Extend Aion's capabilities without recompiling. Drop `.integration` TOML files and Deno JavaScript scripts into the `integrations/` directory, and Aion will dynamically load and execute them via secure subprocesses.
- **Streaming UI**: Enjoy low-latency streaming responses directly in your terminal.
- **Fail-Safe Configurations**: Configurable sliding context windows (`max_history_messages`) and retry limits (`max_tool_errors`) prevent infinite loops and context limit crashes.

## Quickstart

### Prerequisites
1. **Rust**: Make sure you have `cargo` installed.
2. **Deno**: Required to run the default external integrations safely.
3. **LLM Server**: A local inference server running a tool-calling capable model (e.g., Hermes 2 Pro or Hermes 3).
   - *Example*: `llama-cpp-server -m Hermes-3-Llama-3-8B.gguf --port 8080`

### Running Aion

1. Clone the repository and run Aion:
```bash
git clone https://github.com/aion-agent/aion.git
cd aion
cargo run
```

2. On its first boot, Aion will automatically scaffold your workspace:
   - Generating `root.toml` (your main config).
   - Generating `system_prompt.txt` (the hackable system prompt).
   - Initializing the `memory/` SQLite databases.
   - Initializing the `integrations/` directory.

3. Type your prompt in the CLI:
```text
aion: Please search the web for the latest Rust release and save it to my memory.
```

## Documentation

For a deep dive into Aion's mechanics, please refer to the `docs/` directory:
- [Usage Guide & Custom Tools](docs/usage_guide.md)
- [Architecture & Roadmap](docs/architecture_and_roadmap.md)
