# Aion Usage Guide

Aion is designed to be highly configurable and extensible without needing to recompile the core Rust binary. This guide explains how to configure Aion, interact with its memory system, and build your own tools.

---

## 1. Configuration & Startup

When you run Aion for the first time (`cargo run`), it automatically scaffolds your workspace:
- `root.toml`: The primary configuration file.
- `system_prompt.txt`: The hackable system prompt template.
- `memory/`: The directory containing the SQLite databases.
- `integrations/`: The directory where Aion dynamically loads external tools.

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

[integrations]
discover = "./integrations/"
```

- **`base_url`**: Point this to your local `llama.cpp` server or vLLM instance.
- **`max_history_messages`**: Determines Aion's sliding context window. Once this limit is reached, older messages are pruned to prevent context overflow.
- **`max_tool_errors`**: The maximum number of times Aion can consecutively fail a tool call (e.g., hallucinate bad JSON) before it aborts the loop and yields back to you.

### Hackable System Prompt
You can modify `system_prompt.txt` to change Aion's persona or enforce strict rules. Aion automatically searches for the `{{TOOLS}}` placeholder in this file and injects the live JSON schemas of all loaded tools.

---

## 2. The Memory System

Aion uses a native `rusqlite` implementation equipped with `fts5` for full-text search. 

### Core Features
- **Root Store (`root.db`)**: The central repository for long-term facts.
- **Child Scopes**: The agent can dynamically execute the `create_memory_scope` tool to spin up isolated databases (e.g., `project_alpha.db`) inside the `memory/` directory to compartmentalize thoughts.
- **Built-in Memory Tools**:
  - `memory_write`: Stores a key-value fact into a specific scope.
  - `memory_read`: Retrieves an exact fact by key.
  - `memory_search`: Performs a semantic/keyword search across the database.

---

## 3. Creating Custom Tools (Deno Integrations)

Aion dynamically loads custom tools at runtime. By default, we use **Deno** to execute Javascript/Typescript tools securely via subprocesses.

To build a tool, you need three files inside the `integrations/` directory.

### Step 1: Define the Tool Schema (`my_tool_schema.json`)
Provide the standard OpenAI-style JSON schema so Hermes understands how to invoke it.
```json
[
  {
    "name": "greet_user",
    "description": "Greets the user by name.",
    "parameters": {
      "type": "object",
      "properties": {
        "name": { "type": "string" }
      },
      "required": ["name"]
    }
  }
]
```

### Step 2: Map the Integration (`my_tool.integration`)
Link the schema to the actual shell execution command.
```toml
[integration]
name = "greet_tool"
description = "A simple greeting tool"
tools = "./integrations/my_tool_schema.json"
executor = "deno run ./integrations/my_tool.js"
enabled = true
```

### Step 3: Write the Script (`my_tool.js`)
Aion will launch your executor and pipe the LLM's arguments directly into `stdin`. Your script must read `stdin`, process the data, and print the result to `stdout`.

```javascript
// 1. Read from standard input
const input = await new Response(Deno.stdin.readable).text();
const args = JSON.parse(input);

// 2. Execute logic
const message = `Hello there, ${args.name}!`;

// 3. Write back to standard output
console.log(JSON.stringify({ status: "success", message }));
```

That's it! When you restart Aion, the `greet_user` tool will automatically be loaded into the system prompt and ready for use.
