# CLI Reference

## Synopsis

```
pas [OPTIONS] <COMMAND>
```

## Global Options

| Option | Short | Description |
|--------|-------|-------------|
| `--verbose` | `-v` | Enable debug-level logging. Shows detailed handler execution, edge selection decisions, and context updates. |

---

## Commands

### `run` — Execute a pipeline

Parses the DOT file, validates it, and executes each node sequentially. Each `box` node spawns a Claude Code session with the node's prompt.

```
pas run <PIPELINE> [OPTIONS]
```

#### Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `PIPELINE` | Yes | Path to the `.dot` pipeline file |

#### Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--workdir <DIR>` | `-w` | current directory | Working directory for Claude Code sessions. Each node's `claude -p` runs in this directory, so file paths in prompts are relative to it. |
| `--logs <DIR>` | `-l` | `.pas/logs` | Directory for log output. |
| `--dry-run` | — | false | Parse and validate the pipeline without executing any nodes. No Claude Code sessions are spawned, no cost incurred. |
| `--max-budget-usd <AMOUNT>` | — | unlimited | Maximum total spend across all nodes. Pipeline aborts with an error if exceeded. **Strongly recommended for pipelines with loops.** |
| `--max-steps <COUNT>` | — | 200 | Maximum number of node executions before aborting. Prevents runaway loops. A 6-node pipeline that loops 3 times = 18 steps. |

#### Output

Prints:
- Pipeline name and goal
- Working directory (if set)
- Per-node log lines with node ID, label, turns, cost, and error status
- List of completed nodes
- Total cost across all nodes

#### Exit codes

| Code | Meaning |
|------|---------|
| 0 | Pipeline completed successfully |
| 1 | Pipeline failed (validation error, handler error, or goal gate unsatisfied) |

---

### `validate` — Check a pipeline for errors

Runs all 11 lint rules against the pipeline without executing it. Useful for checking syntax and structure before committing a dot file.

```
pas validate <PIPELINE>
```

#### Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `PIPELINE` | Yes | Path to the `.dot` pipeline file |

#### Output

If valid:
```
Pipeline is valid
```

If issues found:
```
[ERROR] StartNodeRule: No start node (Mdiamond) found
[WARN] PromptOnLlmNodesRule: Node 'analyze' has no prompt attribute
```

#### Exit codes

| Code | Meaning |
|------|---------|
| 0 | No errors (warnings are OK) |
| 1 | One or more errors found |

---

### `info` — Inspect a pipeline

Displays the pipeline structure: name, goal, node count, edge count, start/exit nodes, and a list of all nodes with their shapes and types.

```
pas info <PIPELINE>
```

#### Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `PIPELINE` | Yes | Path to the `.dot` pipeline file |

#### Output

```
Pipeline: FixSyncPartialFailure
Goal: Fix baseball-v3-vfd5: sync_player_data silently returns partial results
Nodes: 9
Edges: 9
Start: start (Start)
Exit: done (Done)

Nodes:
  investigate [Investigate Current Behavior] shape=box type=(default)
  implement [Implement Fix] shape=box type=(default)
  verify [Verify Quality] shape=diamond type=(default)
  ...
```

---

### `plan` — Generate PRD or spec documents

Creates a PRD (product requirements document) or technical specification from a template. Optionally uses Claude to generate content from a one-line description.

```
pas plan [OPTIONS]
```

#### Options

| Option | Required | Default | Description |
|--------|----------|---------|-------------|
| `--prd` | One of `--prd`/`--spec` | — | Generate a PRD document |
| `--spec` | One of `--prd`/`--spec` | — | Generate a technical specification |
| `--from-prompt <DESC>` | No | — | Use Claude to generate the document from this description instead of copying the blank template |
| `--output <PATH>` | No | `.pas/prd.md` or `.pas/spec.md` | Output file path |

#### Output

Copies the template or generates content and writes to the output path. Prints next steps for manual editing or beads integration.

#### Examples

```bash
# Copy blank PRD template for manual editing
pas plan --prd

# Generate a PRD from a description
pas plan --prd --from-prompt "Add OAuth2 authentication with Google and GitHub providers"

# Generate a spec to a custom path
pas plan --spec --output docs/specs/auth-spec.md
```

---

### `decompose` — Convert spec to beads issues

Reads a technical specification file and uses Claude to generate beads CLI commands that create an epic, child tasks, and dependencies.

```
pas decompose <SPEC_PATH> [OPTIONS]
```

#### Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `SPEC_PATH` | Yes | Path to the spec markdown file |

#### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--dry-run` | false | Print the generated `bd` commands without executing them |

#### Output

Creates a beads epic with child tasks and dependencies. Prints the epic ID, task count, and dependency count. On `--dry-run`, prints the shell script that would be executed.

#### Examples

```bash
# Preview what would be created
pas decompose .pas/spec.md --dry-run

# Create the epic and tasks
pas decompose .pas/spec.md

# Decompose a spec from a custom path
pas decompose docs/specs/auth-spec.md
```

---

### `scaffold` — Generate pipeline from beads epic

Creates a pipeline DOT file from a beads epic. The pipeline iterates through all child tasks of the epic, implementing each one.

```
pas scaffold <EPIC_ID> [OPTIONS]
```

#### Arguments

| Argument | Required | Description |
|----------|----------|-------------|
| `EPIC_ID` | Yes | Beads epic ID (e.g., `beads-asr`) |

#### Options

| Option | Default | Description |
|--------|---------|-------------|
| `--output <PATH>` | `pipelines/<EPIC_ID>.dot` | Output file path |

#### Output

Generates a DOT pipeline file from the `epic-runner` template with the epic ID substituted. Validates the result and prints node count and validation status.

#### Examples

```bash
# Scaffold a pipeline for an epic
pas scaffold attractor-asr

# Scaffold to a custom path
pas scaffold attractor-asr --output pipelines/auth-feature.dot

# Then run it
pas run pipelines/attractor-asr.dot -w .
```

---

## Examples

### Run with a budget limit (recommended for loops)

```bash
pas run pipelines/epic-runner.dot -w . --max-budget-usd 10.00
```

If total spend across all nodes exceeds $10, the pipeline stops with an error. Prevents a looping pipeline from running up a massive bill overnight.

### Run with a step limit

```bash
pas run pipelines/epic-runner.dot -w . --max-steps 50
```

Limits the pipeline to 50 node executions. For an epic runner with ~7 nodes per loop, this allows ~7 iterations before stopping. The default is 200 steps.

### Run with both limits (safest for unattended runs)

```bash
pas run pipelines/epic-runner.dot -w . --max-budget-usd 20.00 --max-steps 100
```

The pipeline stops at whichever limit is hit first.

### Run a pipeline in your project directory

```bash
pas run pipelines/fix-bug.dot -w .
```

The `-w .` sets the working directory to the current directory. Claude Code can read, edit, and create files relative to this path.

### Run a pipeline for a different project

```bash
pas run ~/pipelines/deploy-check.dot -w ~/projects/my-app
```

The pipeline file and working directory don't need to be in the same place.

### Validate before running

```bash
pas validate pipelines/new-feature.dot && \
pas run pipelines/new-feature.dot -w .
```

Only runs if validation passes.

### Inspect a pipeline to see its structure

```bash
pas info pipelines/epic-runner.dot
```

Quick way to see the nodes and verify the graph shape before running.

### Debug a failing pipeline

```bash
pas -v run pipelines/fix-bug.dot -w .
```

The `-v` flag enables debug logging. You'll see:
- Which handler is selected for each node
- Edge selection decisions (condition evaluation, label matching)
- Context updates after each node
- Goal gate check results

### Dry run to verify parsing

```bash
pas run pipelines/complex-feature.dot --dry-run
```

Parses and validates the pipeline, prints the structure, but doesn't spawn any Claude Code sessions. Zero cost.

### Run from anywhere with an alias

Add to your shell profile (`~/.zshrc` or `~/.bashrc`):

```bash
alias pas='~/.local/bin/pas'
```

Then:

```bash
cd ~/projects/my-app
pas run pipelines/fix-auth.dot -w .
pas validate pipelines/new-feature.dot
pas info pipelines/deploy.dot
```

### Pipeline for a beads issue

```bash
# Look up the issue
bd show baseball-v3-vfd5

# Run the pipeline that fixes it
pas run pipelines/fix-sync-partial-failure.dot -w ~/gt/baseball
```

### Process an entire epic

```bash
# Copy the epic runner template
cp /Volumes/qwiizlab/projects/connect-the-bots/docs/examples/epic-runner.dot pipelines/run-epic.dot

# Replace EPIC_ID with your epic
sed -i '' 's/EPIC_ID/baseball-v3-8xey/g' pipelines/run-epic.dot

# Run it — loops through all child tasks
pas run pipelines/run-epic.dot -w .
```

### Chain validate + run in CI or scripts

```bash
#!/bin/bash
set -e

PIPELINE="$1"
WORKDIR="${2:-.}"

echo "Validating $PIPELINE..."
pas validate "$PIPELINE"

echo "Running $PIPELINE in $WORKDIR..."
pas run "$PIPELINE" -w "$WORKDIR"
```

Usage: `./run-pipeline.sh pipelines/fix-bug.dot ~/projects/my-app`

### Full planning workflow (PRD → Spec → Beads → Pipeline → Execute)

```bash
# Step 1: Generate a PRD from a description
pas plan --prd --from-prompt "Add real-time notifications via WebSockets"

# Step 2: Review and edit .pas/prd.md manually

# Step 3: Generate a spec from a description (or copy template and edit)
pas plan --spec --from-prompt "Add real-time notifications via WebSockets"

# Step 4: Review and edit .pas/spec.md manually

# Step 5: Decompose spec into beads epic + tasks
pas decompose .pas/spec.md

# Step 6: Scaffold pipeline from the epic
pas scaffold <EPIC_ID>

# Step 7: Run the pipeline
pas run pipelines/<EPIC_ID>.dot -w .
```

### Run the meta-pipeline (automated full workflow)

```bash
pas run templates/plan-to-execute.dot -w .
```

The meta-pipeline chains all planning steps with human review gates. It generates PRD, pauses for review, generates spec, pauses for review, decomposes into beads, scaffolds the pipeline, validates, and executes.

### Compare two pipelines

```bash
pas info pipelines/v1.dot
pas info pipelines/v2.dot
```

Quick way to compare node counts and structure between pipeline revisions.

---

## Environment

### Required

- **`claude`** must be in your PATH. The `run` command shells out to `claude -p` for each node. Verify with: `which claude`

### Optional

- **`RUST_LOG`** — Override log level (e.g. `RUST_LOG=debug pas run ...`). The `-v` flag sets this to `debug` automatically.

---

## Node-level Claude Code flags

These are set in the `.dot` file as node attributes and passed through to each `claude -p` invocation:

| Node attribute | Claude CLI flag | Effect |
|----------------|----------------|--------|
| `llm_model` | `--model` | Override model for this node |
| `allowed_tools` | `--allowedTools` | Restrict available tools |
| `max_budget_usd` | `--max-budget-usd` | Cap spending for this node |
| Graph `model` | `--model` (fallback) | Default model when node doesn't specify one |

Every node also gets:
- `--output-format json` — for structured output parsing
- `--no-session-persistence` — each node is a fresh session
- `--dangerously-skip-permissions` — allows file edits and bash execution

### Examples in DOT

```dot
// Cheap read-only investigation using haiku
investigate [
    shape="box"
    llm_model="haiku"
    allowed_tools="Read,Grep,Glob"
    prompt="Find all usages of deprecated_function"
]

// Expensive deep analysis using opus with a budget cap
analyze [
    shape="box"
    llm_model="opus"
    max_budget_usd="5.00"
    prompt="Perform a security audit of the authentication module"
]

// Default model (inherits from graph-level model attribute)
implement [
    shape="box"
    prompt="Fix the SQL injection in the search endpoint"
]
```
