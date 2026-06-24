# Aion Project: Architecture & Roadmap

## 1. System Architecture

Aion is a local AI agent written in Rust, built to orchestrate large language models through persistent hierarchical memory and a dynamic external tool system. It presents itself via a full-screen Ratatui terminal dashboard.

### Source Layout

```
src/
├── main.rs          — Entry point, workspace bootstrap, CLI args, TUI launch
├── config.rs        — root.toml → AppConfig struct deserialization
├── agent_direct.rs  — Core agent: LLM HTTP client, streaming loop, tool dispatch
├── integrations.rs  — Integration loader (.toml files), system prompt builder
├── tui.rs           — Full-screen Ratatui TUI dashboard
├── memory/
│   ├── mod.rs
│   ├── memory.rs    — SQLite memory abstraction (read/write/search/child scopes)
│   ├── migrations/  — SQL schema (memory_kv table, FTS5, child_databases table)
│   └── queries/     — Individual SQL query files
└── tool_call/
    ├── mod.rs
    ├── parser.rs    — XML <tool_call> extractor
    └── executor.rs  — Tool router and async Deno subprocess executor
```

---

## 2. Core Components

### Agent Core (`src/agent_direct.rs`)

The `Agent` struct owns:
- `reqwest::Client` — HTTP client for LLM API calls
- `AgentConfig` — live config (editable from TUI settings)
- `root_memory: Memory` — persistent root SQLite connection
- `active_memory: Option<(String, Memory)>` — currently active child scope (name + connection)
- `integrations: Vec<Integration>` — loaded tool integrations
- `history: Vec<Value>` — conversation history (JSON messages)

`Agent::step(user_input, tx: UnboundedSender<String>)` drives the full tool-use loop:
1. Appends the user message to history.
2. Trims history to `max_history_messages` (sliding window).
3. POSTs to the LLM endpoint with SSE streaming.
4. Forwards visible tokens to the TUI via the `tx` channel.
5. Suppresses output while a `<tool_call>` block is being accumulated.
6. On completion, extracts and executes the tool call, appends `<tool_response>` to history, and loops.
7. Returns the final conversational reply when the LLM responds without a tool call.

### Configuration Parser (`src/config.rs`)

Reads `root.toml` into `AppConfig { agent: AgentConfig, integrations: IntegrationsConfig }`. All fields have sane defaults via `serde` default functions. Supports optional `openrouter_model` and `openrouter_api_key` fields.

### Memory System (`src/memory/`)

SQLite-backed via `rusqlite` with bundled SQLite and FTS5 extension. Schema:
- `memory` table — `(key PK, value, scope, created_at, updated_at)`
- `memory_fts` — FTS5 virtual table mirroring `memory`, kept in sync via INSERT/UPDATE triggers
- `child_databases` table — registry of child scope names, file paths, and creation reasons

**Scope Routing** (implemented in `executor.rs`):
- Writes go to `active_memory` or `root_memory` based on the `scope` argument
- Reads check `active_memory` first, fall back to `root_memory`
- Searches merge results from both connections, tagging each with its source scope

Child databases are full independent SQLite files initialized with the same schema, stored in the `memory/` directory.

### Tool Calling System (`src/tool_call/`)

**Parser** (`parser.rs`): Scans the LLM's raw string output for `<tool_call>...</tool_call>` XML delimiters and deserializes the enclosed JSON into a `ToolCallRequest { name, arguments }`.

**Executor** (`executor.rs`): `async fn execute_tool(...)` dispatches on tool name:
- Built-in memory tools: `memory_write`, `memory_read`, `memory_search`, `create_memory_scope`, `switch_memory_scope`, `list_memory_scopes`
- External integrations: matched by tool name across loaded integrations, spawned as async subprocesses via `tokio::process::Command`

### Integration Loader (`src/integrations.rs`)

`load_all(dir)` scans for `.toml` files in the integrations directory. Each `.toml` manifest is parsed into an `Integration` struct holding its name, description, tool schemas, executor command, and enabled flag.

`build_system_prompt(template, integrations)` replaces the `{{TOOLS}}` placeholder in `system_prompt.txt` with the merged JSON schemas of all built-in memory tools and all enabled integration tools.

### TUI (`src/tui.rs`)

Built with `ratatui` and `crossterm`. Runs entirely inside a `tokio::task::LocalSet` because `rusqlite::Connection` is not `Send` (holds a `RefCell` internally). The `Agent` is wrapped in `Rc<RefCell<Agent>>` and shared between the render path and `spawn_local` step tasks — no `Arc<Mutex>` needed.

**Tabs**:
- **💬 Chat** — Conversation history (scrollable), real-time streaming response display (via `UnboundedReceiver<String>`), input box with `i`/`Esc` focus toggle. Right sidebar shows active memory scope and integration status.
- **🔌 Integrations** — Lists all loaded integrations with enabled/disabled status and tool details. Press `Space` to toggle; change is written back to the `.toml` manifest immediately.
- **⚙️ Settings** — Displays all `AgentConfig` fields. Press `Enter` to edit a field in a centered popup; saves back to `root.toml` on confirm.

---

## 3. The Hermes Tool Calling Protocol

Aion targets models fine-tuned with the Hermes ChatML convention for tool use (Hermes 2 Pro, Hermes 3):

1. **Detection** — The parser scans for `<tool_call>` opening tag in the raw streaming output. Once detected, token forwarding to the TUI is suppressed.
2. **Extraction** — After `</tool_call>` is seen, the JSON body is parsed into a `ToolCallRequest`.
3. **Execution** — `execute_tool` dispatches synchronously for built-in memory ops, or spawns an async subprocess for Deno integrations.
4. **Response Feed** — The tool result is JSON-serialized and injected into history as a `<tool_response>` user message.
5. **Recursive Loop** — The LLM is re-invoked until it replies conversationally (no tool call detected), or until `max_tool_errors` consecutive parse failures trigger an early abort.

---

## 4. Async Deno Subprocess Execution

When an external integration tool is invoked:

1. Aion spawns `sh -c <executor_command>` using `tokio::process::Command` (fully async).
2. The JSON-serialized tool arguments are piped into the child process's `stdin`.
3. The child process (Deno script) reads `stdin`, runs its logic, and writes a JSON result to `stdout`.
4. Aion awaits `child.wait_with_output()` and parses the `stdout` as JSON to return to the agent history.

The Deno runtime's permission flags (`--allow-read`, `--allow-run`, `--allow-net`, etc.) provide a security boundary for each integration.

---

## 5. Roadmap

| Task | Status |
|---|---|
| Streaming responses to TUI via async channels | ✅ Done |
| Hierarchical child memory scopes (create/switch/list/route) | ✅ Done |
| Ratatui full-screen TUI (Chat + Integrations + Settings) | ✅ Done |
| Live integration toggling with `.toml` file write-back | ✅ Done |
| Live settings editing with `root.toml` write-back | ✅ Done |
| Async Deno subprocess execution (`tokio::process::Command`) | ✅ Done |
| OpenRouter cloud inference support | ✅ Done |
| `.toml` integration manifest extension | ✅ Done |
| `.integration` → `.toml` auto-migration on boot | ✅ Done |
| Sliding context window (max_history_messages) | ✅ Done |
| Error circuit breaker (max_tool_errors) | ✅ Done |
| **Vector memory (Two-tier RAM + Disk)** | ⬜ Planned — see `vector_memory_roadmap.md` |
| **Robust LLM error self-correction** | ⬜ Planned |
| **Multi tool-call per turn** | ⬜ Planned |
| **`memory_commit` tool (RAM → Disk promotion)** | ⬜ Planned |
| **Env var support for API keys** | ⬜ Planned |
