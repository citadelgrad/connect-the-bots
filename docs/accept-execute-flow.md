# Accept & Execute Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│  BROWSER (WASM)                                                     │
│                                                                     │
│  ┌──────────────────┐                                               │
│  │ "Approve &       │  on_click                                     │
│  │  Execute" button  ├──────┐                                       │
│  └──────────────────┘      │                                        │
│                             ▼                                       │
│                   ┌──────────────────┐                               │
│                   │ ApprovalBar      │                               │
│                   │ set_phase(       │                               │
│                   │   Decomposing)   │                               │
│                   │ dispatch action  │                               │
│                   └────────┬─────────┘                               │
│                            │                                        │
│                            │ POST /api/start_execution{hash}        │
│                            │ (Leptos server fn)                     │
└────────────────────────────┼────────────────────────────────────────┘
                             │
                             ▼
┌─────────────────────────────────────────────────────────────────────┐
│  SERVER (SSR / Axum)                                                │
│                                                                     │
│  ┌──────────────────────────────────────┐                           │
│  │ start_execution()                     │                          │
│  │                                       │                          │
│  │  1. Verify .attractor/spec.md exists  │                          │
│  │                                       │                          │
│  │  2. ┌──────────────────────────┐      │                          │
│  │     │ $ attractor decompose   │      │                          │
│  │     │   .attractor/spec.md    │      │                          │
│  │     └───────────┬──────────────┘      │                          │
│  │                 │ stdout               │                          │
│  │                 ▼                      │                          │
│  │     Parse "Epic ID: attractor-xxx"    │                          │
│  │                 │                      │                          │
│  │  3. ┌──────────▼───────────────┐      │                          │
│  │     │ $ attractor scaffold    │      │                          │
│  │     │   <epic-id>             │      │                          │
│  │     └───────────┬──────────────┘      │                          │
│  │                 │                      │                          │
│  │                 ▼                      │                          │
│  │     Read pipelines/<epic-id>.dot      │                          │
│  │                 │                      │                          │
│  │  4. Parse .dot → PipelineGraph        │                          │
│  │                 │                      │                          │
│  │  5. ┌───────────▼──────────────────┐  │                          │
│  │     │ tokio::spawn background     │  │                          │
│  │     │ run_pipeline_with_streaming │  │                          │
│  │     └───────────┬──────────────────┘  │                          │
│  │                 │                      │                          │
│  │  Return ExecutionResponse             │                          │
│  │  { session_id, epic_id,              │                          │
│  │    pipeline_path }                    │                          │
│  └──────────┬───────────────┬────────────┘                          │
│             │               │                                       │
│             │               ▼ (background task)                     │
│             │    ┌───────────────────────────────────┐              │
│             │    │ run_pipeline_with_streaming()      │              │
│             │    │                                    │              │
│             │    │  Initialize Context (workdir,      │              │
│             │    │    graph attrs)                    │              │
│             │    │         │                          │              │
│             │    │         ▼                          │              │
│             │    │  ┌─────────────────────┐           │              │
│             │    │  │ For each node:      │           │              │
│             │    │  │                     │           │              │
│             │    │  │  publish_event ───────────► SSE broadcast     │
│             │    │  │  { type:node_start, │     channel              │
│             │    │  │    node_id, label } │     (in-memory)          │
│             │    │  │         │           │           │              │
│             │    │  │  Execute handler    │           │              │
│             │    │  │  (registry lookup)  │           │              │
│             │    │  │         │           │           │              │
│             │    │  │  publish_event ───────────► SSE broadcast     │
│             │    │  │  { type:node_complete│     channel             │
│             │    │  │    status, cost,    │           │              │
│             │    │  │    notes }          │           │              │
│             │    │  │         │           │           │              │
│             │    │  │  Apply context      │           │              │
│             │    │  │  updates            │           │              │
│             │    │  │         │           │           │              │
│             │    │  │  Select next edge   │           │              │
│             │    │  │  (or terminal)      │           │              │
│             │    │  └─────────┬───────────┘           │              │
│             │    │            │                       │              │
│             │    │            ▼                       │              │
│             │    │  publish_event ────────────► SSE broadcast       │
│             │    │  { type:pipeline_complete,   channel              │
│             │    │    total_cost,                                    │
│             │    │    completed_nodes }                              │
│             │    │            │                       │              │
│             │    │  clear_session_state()             │              │
│             │    └───────────────────────────────────┘              │
│                                                                     │
│  SSE Endpoint: GET /api/stream/{session_id}                         │
│  ┌──────────────────────────────────────┐                           │
│  │ stream_events()                       │                          │
│  │  - Replays buffered events on         │                          │
│  │    reconnect (state_sync)             │                          │
│  │  - Forwards broadcast → SSE           │                          │
│  │  - KeepAlive pings                    │                          │
│  └──────────────────┬───────────────────┘                           │
│                     │                                               │
└─────────────────────┼───────────────────────────────────────────────┘
                      │ SSE stream
                      ▼
┌─────────────────────────────────────────────────────────────────────┐
│  BROWSER (WASM) — after server fn returns                           │
│                                                                     │
│  ApprovalBar.Effect                                                 │
│    on_approve(session_id) ──► switches view to ExecutionPanel       │
│                                                                     │
│  ┌──────────────────────────────────────┐                           │
│  │ ExecutionPanel                        │                          │
│  │                                       │                          │
│  │  Connects to /api/stream/{session_id} │                          │
│  │  via EventSource (SSE)                │                          │
│  │         │                             │                          │
│  │         ▼                             │                          │
│  │  process_event() for each SSE msg:    │                          │
│  │                                       │                          │
│  │  "node_start"                         │                          │
│  │    → add node to list (InProgress)    │                          │
│  │                                       │                          │
│  │  "node_complete"                      │                          │
│  │    → update node status               │                          │
│  │      (Success/Failed/Skipped)         │                          │
│  │    → update cost display              │                          │
│  │                                       │                          │
│  │  "pipeline_complete"                  │                          │
│  │    → set is_running = false           │                          │
│  │    → show "Done" badge                │                          │
│  │                                       │                          │
│  │  "error"                              │                          │
│  │    → show error message               │                          │
│  │    → set is_running = false           │                          │
│  └──────────────────────────────────────┘                           │
│                                                                     │
│  Renders: ExecutionNode components                                  │
│    ┌────────┐ ┌────────┐ ┌────────┐                                │
│  │ Node 1 │ │ Node 2 │ │ Node 3 │ ...                            │
│  │ ✓ Done │ │ ⟳ Run  │ │ · Wait │                                │
│  │ $0.02  │ │        │ │        │                                 │
│  └────────┘ └────────┘ └────────┘                                  │
└─────────────────────────────────────────────────────────────────────┘
```

## Sequence Summary

1. **Click** → `ApprovalBar` dispatches Leptos server fn `start_execution()`
2. **Server** verifies `.attractor/spec.md` exists
3. **Decompose** → `$ attractor decompose .attractor/spec.md` → parses epic ID
4. **Scaffold** → `$ attractor scaffold <epic-id>` → generates pipeline `.dot` file
5. **Parse** → reads `.dot` file, builds `PipelineGraph`
6. **Spawn** → background tokio task runs `run_pipeline_with_streaming()`
7. **Return** → `ExecutionResponse { session_id, epic_id, pipeline_path }`
8. **Browser** receives response, switches to `ExecutionPanel`, opens SSE to `/api/stream/{session_id}`
9. **Pipeline loop** → for each graph node: resolve handler → execute → publish `node_start`/`node_complete` via broadcast channel → select next edge
10. **SSE relay** → `stream_events()` bridges broadcast channel to SSE, with reconnect replay
11. **Complete** → publishes `pipeline_complete`, clears session state
