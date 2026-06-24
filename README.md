# Aion — Local AI Agent

Aion is a modular, local-first AI agent written in Rust. It connects to any OpenAI-compatible LLM endpoint (e.g. `llama.cpp` serving Hermes models) and provides a full-screen terminal dashboard with persistent hierarchical memory and dynamically loaded tool integrations.

## Features

- **Full-screen Ratatui TUI** — An interactive terminal dashboard with three tabs: Chat, Integrations Manager, and Settings Editor. No plain REPL — everything is managed from within the TUI.
- **Hermes Tool Calling Protocol** — A custom ChatML-style XML parser that streams LLM tokens to the chat view while silently buffering and executing `<tool_call>` XML blocks in the background.
- **Hierarchical Memory Scopes** — Built on `rusqlite` + FTS5, Aion maintains a persistent `root.db` and can create and switch between isolated child databases for compartmentalized reasoning per project or task. Scope switching is a live, tool-driven operation.
- **Dynamic Deno Integrations** — Extend Aion without recompiling. Drop a `.toml` manifest and a Deno JS/TS script into `integrations/` and it is auto-loaded on next boot. Toggle integrations on/off live from the TUI.
- **Async Streaming** — LLM tokens stream in real-time into the TUI chat view via `tokio::sync::mpsc` channels. External tool subprocesses are fully async via `tokio::process::Command`.
- **OpenRouter Support** — Switch between local and cloud inference with `--openrouter` flag or configure it in `root.toml`.
- **Fail-Safe Loop** — Configurable `max_history_messages` (sliding context window) and `max_tool_errors` (error circuit breaker) prevent runaway loops.

## Quickstart

### Prerequisites

1. **Rust** — Install via [rustup.rs](https://rustup.rs)
2. **Deno** — Required for the default external integrations. Install via [deno.land](https://deno.land)
3. **LLM Server** — A local OpenAI-compatible inference server running a tool-calling capable model.
   - *Example*: `llama-server -m Hermes-3-Llama-3.1-8B.gguf --port 8080`
   - Or use OpenRouter (cloud) with the `--openrouter` flag.

### Running Aion

```bash
git clone https://github.com/aion-agent/aion.git
cd aion
cargo run
```

On first boot, Aion auto-scaffolds your workspace:
- `root.toml` — your main configuration file
- `system_prompt.txt` — the hackable system prompt template
- `memory/root.db` — the persistent SQLite memory store
- `integrations/` — directory for dynamic tool integrations (seeded with `fs`, `shell`, `weather`, `web`)

### CLI Flags

```
-m, --model <MODEL>           Override the model name
-o, --openrouter              Use OpenRouter API instead of local endpoint
-c, --config <FILE>           Use a custom config file (default: root.toml)
-i, --integrations <DIR>      Use a custom integrations directory
```

## TUI Navigation

| Key | Action |
|---|---|
| `1` / `2` / `3` | Switch to Chat / Integrations / Settings tab |
| `Tab` / `←` `→` | Cycle between tabs |
| `i` | Focus the chat input box |
| `Esc` | Unfocus input — scroll chat history |
| `↑` / `↓` | Scroll history or navigate lists |
| `Space` / `Enter` | Toggle integration on/off (Integrations tab) or edit field (Settings tab) |
| `Ctrl+C` | Exit and restore terminal |

## Documentation

- [Usage Guide & Custom Tools](docs/usage_guide.md)
- [Architecture & Roadmap](docs/architecture_and_roadmap.md)
- [Vector Memory Roadmap](docs/vector_memory_roadmap.md)
