# Attractor - Agent Instructions

## Versioning

All crates share a single version defined in the workspace root `Cargo.toml`:

```toml
[workspace.package]
version = "0.1.0"
```

Each crate inherits it via `version.workspace = true` in its own `Cargo.toml`. **Do not set versions directly in individual crates.**

### How to bump the version

1. Edit the `version` field in `/Cargo.toml` under `[workspace.package]`
2. Run `cargo check` to verify the workspace compiles
3. Rebuild the release binary: `cargo build --release`
4. Commit the `Cargo.toml` change and tag the release:
   ```bash
   git add Cargo.toml
   git commit -m "Bump version to X.Y.Z"
   git tag vX.Y.Z
   ```

### When to bump the version

- **Patch** (0.1.x): Bug fixes, documentation changes, internal refactors
- **Minor** (0.x.0): New features, new handlers, new CLI subcommands
- **Major** (x.0.0): Breaking changes to pipeline format, CLI interface, or public API

### Verifying the version

```bash
cargo run -- --version
```

The CLI uses clap's `#[command(version)]` attribute, which reads the version from `attractor-cli/Cargo.toml` (inherited from workspace) at compile time.
