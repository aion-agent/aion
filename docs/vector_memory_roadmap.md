# Vector Database Roadmap: Two-Tier Memory Architecture

Currently, Aion relies entirely on a SQLite "database-of-databases" architecture. While excellent for rigid, persistent storage, it requires the LLM to consciously call tools (`memory_read`, `memory_search`) to retrieve context. 

The next evolution of Aion is a **Two-Tier Memory System**, where a Vector Database acts as the agent's "RAM" (immediate, fluid context) and SQLite acts as its "Hard Drive" (persistent storage).

---

## The Vision
1. **Tier 1 (RAM): The In-Memory Vector Store**
   - High-speed semantic storage holding recent interactions, active thoughts, and transient context.
   - Automatically queried in the background on every user turn. The top relevant chunks are silently injected into the system prompt, providing instant "intuition" without requiring a tool call.
2. **Tier 2 (Disk): The SQLite Databases**
   - The current architecture. Used for permanent records, rigid key-value facts, and long-term project files.
   - Requires explicit tool calls to read/write.

---

## Implementation Phases

### Phase 1: Local Embedding Engine Integration
To store vectors, Aion needs to convert text into embeddings locally.
- **Approach A (API-based)**: Utilize the `llama.cpp` embeddings endpoint if the user is running an embedding model alongside Hermes.
- **Approach B (Native Rust)**: Integrate a lightweight Rust ML library like `candle` or `ort` (ONNX Runtime) to run a tiny, hyper-fast embedding model (e.g., `all-MiniLM-L6-v2` or `nomic-embed-text`) directly inside the Aion binary.

### Phase 2: Building the Vector "RAM"
- Integrate a lightweight vector index in Rust. Since this is meant to be volatile RAM, we do not need a heavy standalone server like Qdrant or Milvus. 
- **Tooling**: We can use `hnswlib-rs` (Highly robust in-memory HNSW graphs) or an extension like `sqlite-vec` if we want to keep the stack purely SQLite-based but entirely in-memory (`:memory:`).
- **Injection Pipeline**: Modify `agent_direct.rs`. Before calling the LLM, Aion will embed the user's latest message, fetch the Top-K relevant vectors from RAM, and append them as a hidden context block in the system prompt.

### Phase 3: The Write-Through Cache (Promotion / Demotion)
- **Automatic Ingestion**: Every time the user speaks or a tool returns a result, the text is chunked, embedded, and dropped into the Vector RAM.
- **Context Pruning (Forgetting)**: As the Vector RAM grows, older or less frequently accessed vectors are decayed or removed to maintain speed and relevance.
- **Committing to Disk**: We will introduce a new tool, `memory_commit`. The LLM can decide that a concept currently in its "RAM" is important enough to be saved forever. It will call this tool to write the synthesized fact into the SQLite root database.

### Phase 4: Agent Tool Refactoring
With the background vector injection handling most semantic recall, the explicit memory tools will be refactored:
- `memory_search` (SQLite) will remain for deep-diving into cold storage.
- `memory_write` will be used strictly for hard facts and configurations.
- `memory_recall` (New) will allow the LLM to explicitly search its Vector RAM for things that didn't automatically trigger in the background context injection.

---

## Summary of Tech Stack Additions
- **Embeddings**: `candle-core` or `ort` (if native), or `reqwest` (if calling `llama.cpp` embeddings).
- **Vector Index**: `hnswlib-rs` or simple cosine similarity on `Vec<f32>` (since "RAM" will likely be small enough for brute-force matrix multiplication to be instantaneous).
