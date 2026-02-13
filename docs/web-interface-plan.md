# Attractor Web Interface — Implementation Plan

## Context

Attractor is a DOT-based pipeline runner for AI workflows. You're building a parallel workflow on another branch that generates PRDs and spec files, decomposes them into beads, and creates `.dot` pipeline files. This web interface provides a visual layer for that planning process — a place to prompt, review, iterate on PRD/spec documents, and then kick off pipeline execution with live streaming progress.

The repo is a git worktree of the main attractor project on `feature/interface-web`.

## Architecture

**Leptos fullstack app** — single Rust crate that compiles to both server (Axum) and client (WASM). Uses `cargo-leptos` as the build tool.

### New Crate: `attractor-web`

```
crates/attractor-web/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # App definition, hydrate entry
│   ├── main.rs                   # Server binary (Axum)
│   ├── app.rs                    # Router: /, /editor, /execute
│   ├── state.rs                  # AppState (sessions, broadcast channels)
│   ├── pages/
│   │   ├── mod.rs
│   │   ├── prompt.rs             # Chat input → LLM generates PRD/spec
│   │   ├── editor.rs             # Two-pane markdown editor (PRD | Spec)
│   │   └── execution.rs          # Pipeline execution with live streaming
│   ├── components/
│   │   ├── mod.rs
│   │   ├── chat_input.rs         # Multi-line prompt input
│   │   ├── markdown_pane.rs      # Textarea + live preview
│   │   └── node_status.rs        # Pipeline node progress indicator
│   └── server/
│       ├── mod.rs
│       ├── generate.rs           # Server fn: prompt → PRD + spec via claude CLI
│       ├── execute.rs            # Server fn: start pipeline, return session_id
│       ├── stream.rs             # SSE endpoint: bridge PipelineEvents to browser
│       └── session.rs            # Session tracking, checkpoint/resume
├── style/
│   └── main.scss
└── public/
```

### Data Flow

```
1. User types prompt at /
2. Server calls: claude -p "<system+user prompt>" --output-format stream-json
   → Streams partial content back to browser via server fn
   → Accumulates final result, splits into PRD + Spec sections
3. Navigate to /editor with PRD (left pane) and Spec (right pane)
4. User edits both documents freely, iterates
5. User clicks "Execute" → server fn:
   - Generates session_id (UUID v4)
   - Writes PRD + Spec to workdir/.attractor/
   - Spawns pipeline via PipelineExecutor with EventEmitter
   - Returns session_id
6. Browser opens EventSource to /api/stream/{session_id}
   → EventEmitter.subscribe() → SSE events to browser
   → ExecutionPage shows node progress, streaming output, cost
```

### Streaming: Two Layers

**Layer 1: Claude CLI → Server** (for PRD/spec generation)
- `claude -p "..." --output-format stream-json --include-partial-messages`
- Read stdout line-by-line as NDJSON
- Each line has `type` field: `system`, `assistant` (text deltas), `result`
- Stream text deltas to browser as they arrive

**Layer 2: Pipeline Events → Browser** (for execution)
- The existing `EventEmitter` (broadcast channel) in `events.rs` already emits `PipelineEvent` variants: `StageStarted`, `StageCompleted`, `EdgeSelected`, etc.
- SSE endpoint subscribes to the emitter and forwards events as JSON
- Browser `EventSource` receives and renders them

### Modifications to Existing Crates

**`attractor-pipeline/src/handlers/mod.rs`** — CodergenHandler changes:
- Add `CodergenConfig` to support `--output-format stream-json` + `--session-id`
- When `stream-json` mode: read stdout line-by-line, parse NDJSON, forward content deltas to an optional `broadcast::Sender`, accumulate final `result` for Outcome
- When `json` mode (default): unchanged behavior, backward compatible
- Add `--session-id <uuid>` flag instead of `--no-session-persistence` when a session_id is provided

**`attractor-pipeline/src/checkpoint.rs`** — Add `session_id: Option<String>` to `PipelineCheckpoint` for resume across checkpoint/restore cycles.

**`attractor-pipeline/src/engine.rs`** — Accept optional `EventEmitter` in `PipelineExecutor` (may already be wired; the web crate will construct one and subscribe).

### Key Dependencies

```toml
leptos = "0.7"
leptos_axum = "0.7"
leptos_router = "0.7"
leptos_meta = "0.7"
axum = "0.8"
pulldown-cmark = "0.12"     # Markdown rendering (compiles to WASM)
tokio-stream = "0.1"        # BroadcastStream for SSE
# Plus workspace deps: tokio, serde, serde_json, uuid, tracing
# Plus internal: attractor-types, attractor-pipeline, attractor-dot (server-only)
```

### Session ID & Checkpoint/Resume

1. `start_pipeline()` generates a UUID v4 session_id
2. Stored in `AppState.sessions: HashMap<String, SessionState>`
3. `SessionState` holds: session_id, EventEmitter, pipeline status, checkpoint path
4. CodergenHandler passes `--session-id <uuid>` to claude CLI (instead of `--no-session-persistence`)
5. If browser disconnects and reconnects to `/api/stream/{session_id}`, it re-subscribes to the existing broadcast channel
6. If the server restarts, `load_checkpoint()` restores state and the same session_id resumes the claude conversation

## Implementation Phases

### Phase 1: Scaffold (get pixels on screen)
- Add `crates/attractor-web` to workspace
- Set up Leptos + Axum with `cargo-leptos`
- Three routes with placeholder content: `/`, `/editor`, `/execute`
- `ChatInput` component on `/`
- Two side-by-side `MarkdownPane` components on `/editor` (hardcoded content)
- Verify `cargo leptos watch` works

### Phase 2: PRD/Spec Generation
- `generate_prd_spec` server function in `server/generate.rs`
- Shell out to `claude -p` with a system prompt that produces structured PRD + Spec
- Start with `--output-format json` (simpler; streaming comes in Phase 3)
- Wire prompt page → server fn → navigate to editor with results

### Phase 3: Streaming Infrastructure
- Modify `CodergenHandler` with `CodergenConfig` for `stream-json` support
- Add `session_id` to `PipelineCheckpoint`
- SSE endpoint at `/api/stream/{session_id}`
- Upgrade PRD generation to use `stream-json` for live typing effect

### Phase 4: Pipeline Execution
- `start_pipeline` server function
- Wire "Execute" button → start pipeline → open SSE stream
- `ExecutionPage` renders node progress, content, cost in real time

### Phase 5: Resume & Polish
- Session resume on browser reconnect
- Checkpoint-based resume on server restart
- Styling, loading states, error handling

## Verification

1. `cargo leptos watch` — app builds and renders at localhost:3000
2. Type a prompt → PRD + Spec appear in two-pane editor
3. Edit both documents, click Execute
4. Execution page shows live node progress via SSE
5. Disconnect browser, reconnect → stream resumes
6. `cargo test --workspace` — all existing tests still pass
