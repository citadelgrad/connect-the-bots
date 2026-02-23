# Attractor - Agent Instructions

## Build & Test

```bash
cargo build --release          # Build CLI binary
cargo test                     # Run all tests
cargo test -p attractor-dot    # Test a single crate
cargo clippy --workspace       # Lint
cargo fmt --all -- --check     # Format check
```

The CLI binary is `attractor`. Install with `cargo install --path crates/attractor-cli`.

## Versioning

All crates share a single version in workspace root `Cargo.toml` under `[workspace.package]`. Each crate inherits via `version.workspace = true`. **Never set versions directly in individual crates.** Bump only in the workspace root, then run `cargo check`.

## Key Gotchas

- The default `codergen` handler shells out to the local `claude` CLI — it requires Claude Code installed, no API key needed
- Direct LLM handlers (OpenAI/Anthropic/Gemini) need their respective `*_API_KEY` env vars
- Pipeline files use DOT (Graphviz digraph) syntax — see `docs/guide.md` for node attributes
- Integration tests are in `crates/attractor-pipeline/tests/integration.rs`

## Docs Reference

| Doc | Contents |
|-----|----------|
| `docs/guide.md` | DOT syntax, node attributes, pipeline patterns, planning workflow |
| `docs/cli-reference.md` | CLI commands, flags, environment setup |
| `docs/task-verification.md` | Handler dispatch, goal gates, edge routing, budget guards |
