# Aion Usage Guide

Aion is designed to be highly configurable and extensible without needing to recompile the core binary. This guide explains how to configure Aion, interact with its memory system, manage the TUI, and build your own tool integrations.

---

## 1. Configuration & Startup

When you run Aion for the first time (`cargo run`), it automatically scaffolds your workspace:
- `root.toml` — primary configuration file
- `system_prompt.txt` — the LLM system prompt template
- `memory/` — directory containing SQLite databases
- `integrations/` — directory where Aion dynamically loads external tools

> **Migration note**: Any existing `.integration` files in your `integrations/` directory are automatically renamed to `.toml` on startup.

### `root.toml` Parameters

```toml
[agent]
name = "aion"
model = "hermes-3"
memory = "./memory/root.db"
temperature = 0.6
max_tokens = 1024
base_url = "http://localhost:8080"
max_history_messages = 20
max_tool_errors = 3
system_prompt = "system_prompt.txt"

# Optional: use OpenRouter cloud inference
# openrouter_model = "meta-llama/llama-3.1-8b-instruct:free"
# openrouter_api_key = "your-key-here"

[integrations]
discover = "./integrations/"
```

| Field | Description |
|---|---|
| `base_url` | Local LLM server URL (e.g. `http://localhost:8080`) |
| `model` | Model name passed to the LLM API |
| `temperature` | LLM sampling temperature |
| `max_tokens` | Maximum tokens per LLM response |
| `max_history_messages` | Sliding context window size — older messages are pruned beyond this limit |
| `max_tool_errors` | Consecutive tool failures before the agent yields back to the user |
| `openrouter_model` | Cloud model name for OpenRouter (optional) |
| `openrouter_api_key` | OpenRouter API key (optional) |

### Hackable System Prompt

Edit `system_prompt.txt` to change Aion's persona, constraints, or instructions. The `{{TOOLS}}` placeholder is automatically replaced at runtime with the JSON schemas of all loaded integrations.

### Settings Tab (TUI)

You can edit all `root.toml` parameters live from the **Settings** tab in the TUI without restarting Aion. Navigate with `↑`/`↓`, press `Enter` to edit a field, and confirm with `Enter` again. Changes are saved back to `root.toml` immediately.

---

## 2. The Memory System

Aion uses a native `rusqlite` implementation with `FTS5` full-text search. Memory is hierarchical: a root database for persistent long-term facts, and child databases for isolated per-project scopes.

### Built-in Memory Tools

The LLM has access to six built-in memory tools:

| Tool | Description |
|---|---|
| `memory_write` | Write a key-value fact. Scope parameter: `"root"` or `"current"` (active scope). |
| `memory_read` | Read by key. Checks the active scope first, falls back to root. |
| `memory_search` | FTS5 full-text search. Returns results from both the active scope and root, labeled by source. |
| `create_memory_scope` | Creates a new isolated child SQLite database in `memory/`. Registered in root. |
| `switch_memory_scope` | Switches the active scope to a named child database, or back to `"root"`. |
| `list_memory_scopes` | Lists all registered child scopes with their active status. |

### Scope Routing Behaviour

- **`memory_write` with `scope = "current"`** — Writes to the currently active child scope. If no scope is active, falls back to root.
- **`memory_write` with `scope = "root"`** — Always writes to root regardless of active scope.
- **`memory_read`** — Checks the active scope first. If the key is not found, automatically checks root.
- **`memory_search`** — Queries both the active scope and root; merged results are returned with a `"scope"` field indicating origin.

### Example Agent Workflow

```
User:  Create a scope for my Rust project
Agent: <tool_call> {"name": "create_memory_scope", "arguments": {"name": "rust_project", "reason": "isolate Rust notes"}} </tool_call>
       <tool_call> {"name": "switch_memory_scope", "arguments": {"name": "rust_project"}} </tool_call>
       Switched to scope: rust_project

User:  Remember that we use ratatui 0.26
Agent: <tool_call> {"name": "memory_write", "arguments": {"key": "tui_crate", "value": "ratatui 0.26", "scope": "current"}} </tool_call>
       Saved to scope: rust_project
```

### Chat Sidebar

While in the **Chat** tab, the right sidebar shows the currently active memory scope and the enabled/disabled status of all loaded integrations at a glance.

---

## 3. Creating Custom Tool Integrations

Aion dynamically loads tools from the `integrations/` directory at startup. Each integration consists of three files:

### Step 1: Define the Tool Schema (`my_tool_tools.json`)

Standard OpenAI-style JSON schema. This is injected verbatim into the system prompt so the LLM knows how to call it.

```json
[
  {
    "name": "greet_user",
    "description": "Greets the user by name.",
    "parameters": {
      "type": "object",
      "properties": {
        "name": { "type": "string", "description": "The user's name" }
      },
      "required": ["name"]
    }
  }
]
```

### Step 2: Create the Integration Manifest (`my_tool.toml`)

> **Note**: Manifests must use the `.toml` extension. Files with `.integration` extensions are automatically migrated on startup.

```toml
[integration]
name = "greet_tool"
description = "A simple greeting integration"
tools = "./integrations/my_tool_tools.json"
executor = "deno run --allow-net ./integrations/my_tool.js"
enabled = true
```

| Field | Description |
|---|---|
| `name` | Internal identifier for the integration |
| `description` | Short description shown in the TUI Integrations tab |
| `tools` | Path to the JSON tool schema file |
| `executor` | Shell command used to run the tool script |
| `enabled` | Whether the integration is active. Can be toggled from the TUI. |

### Step 3: Write the Script (`my_tool.js`)

Aion pipes the LLM's JSON arguments into `stdin`. Your script reads from `stdin` and writes the result to `stdout` as JSON.

```javascript
// Read arguments from stdin
const input = await new Response(Deno.stdin.readable).text();
const args = JSON.parse(input);

// Execute logic
const message = `Hello there, ${args.name}!`;

// Return result to Aion via stdout
console.log(JSON.stringify({ status: "success", message }));
```

On restart, `greet_user` is automatically loaded into the system prompt and available for the LLM to call.

### Managing Integrations in the TUI

Switch to the **Integrations** tab (`2`). Navigate with `↑`/`↓`. Press `Space` or `Enter` to toggle the selected integration's `enabled` state — this is immediately written back to its `.toml` file.

---

## 4. OpenRouter (Cloud Inference)

To use a cloud model instead of a local server, run:

```bash
cargo run -- --openrouter -m meta-llama/llama-3.1-8b-instruct:free
```

Or configure it persistently in `root.toml`:

```toml
[agent]
openrouter_model = "meta-llama/llama-3.1-8b-instruct:free"
openrouter_api_key = "sk-or-..."
```

Then run with:

```bash
cargo run -- --openrouter
```

The API key can also be edited live from the **Settings** tab in the TUI.
