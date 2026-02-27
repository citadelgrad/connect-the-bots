# GitHub Actions Debugging Skill

**Date:** 2026-02-26
**Status:** Brainstorm complete

## What We're Building

A global Claude Code skill (`/gha`) that helps debug failing GitHub Actions runs and author new workflows. Installed at `~/.claude/commands/gha.md`, it works in any repo with GitHub Actions.

### Modes

| Mode | Trigger | What it does |
|------|---------|-------------|
| **Debug** (default) | `/gha` or `/gha <run-id>` | Fetches failed run logs via `gh`, analyzes errors, reads workflow YAML + referenced code, suggests fixes |
| **Create** | `/gha create <name>` | Guided workflow authoring with questions about triggers, jobs, steps |
| **Fix** | `/gha fix <file>` | Reads an existing workflow YAML, identifies issues, applies fixes |

### Smart Defaults

- No args: auto-detects the most recent failed run via `gh run list --status=failure --limit=1`
- If no failures found: lists workflows and offers to create/edit
- Explicit run ID or file path overrides auto-detection

## Why This Approach

- **Single file, single command:** YAGNI. One SKILL.md handles routing via prompt logic. Split later if it grows unwieldy.
- **Global skill:** GitHub Actions debugging is repo-agnostic. Installing in `~/.claude/commands/` makes it available everywhere.
- **`gh` CLI as backbone:** Already authenticated, handles API pagination, supports all needed operations (`gh run view`, `gh run view --log-failed`, `gh api`).
- **No subagents initially:** Simple sequential analysis is sufficient for most CI failures. Parallel fan-out for matrix builds can be added later.

## Key Decisions

| # | Decision | Rationale |
|---|----------|-----------|
| 1 | Single `/gha` skill, not a family of commands | Simpler, auto-detection across modes, one file to maintain |
| 2 | Global install (`~/.claude/commands/`) | Works in any repo, not tied to a specific project |
| 3 | Smart defaults with override args | Reduces friction for common case (debug latest failure) while keeping explicit control available |
| 4 | `gh` CLI for all GitHub API access | Already installed/authenticated, no custom API code needed |
| 5 | No subagent fan-out in v1 | YAGNI - sequential analysis handles 90% of cases, can add parallel analysis later |

## Capabilities (v1)

### Debug Mode
- Fetch failed run logs (`gh run view --log-failed`)
- Parse error output to identify root cause category (syntax, dependency, test, permissions, timeout, etc.)
- Read the workflow YAML that triggered the failure
- Read referenced source files if the failure is in application code
- Suggest concrete fixes (edit workflow YAML or source code)

### Create Mode
- Ask about trigger events (push, PR, schedule, manual)
- Ask about job structure (build, test, deploy)
- Ask about runner OS, language/framework
- Generate workflow YAML following GitHub Actions best practices
- Write to `.github/workflows/<name>.yml`

### Fix Mode
- Read and validate existing workflow YAML
- Check for common issues (deprecated actions, pinned vs floating versions, missing permissions, inefficient caching)
- Apply fixes directly

## Key `gh` Commands Used

```bash
# List recent runs
gh run list --limit=10
gh run list --status=failure --limit=5

# View specific run
gh run view <run-id>
gh run view <run-id> --log-failed

# View workflow files
gh workflow list
gh workflow view <workflow-id>

# API for advanced queries
gh api repos/{owner}/{repo}/actions/runs/{run_id}/jobs
```

## Open Questions

1. **Should the skill auto-apply fixes or just suggest them?** Leaning toward suggesting with a confirmation prompt.
2. **How deep should log analysis go?** Full log can be huge. Probably focus on `--log-failed` output and parse the last N lines of each failed step.
3. **Should it handle self-hosted runner issues?** Probably not in v1 - stick to GitHub-hosted runners.

## Out of Scope (v1)

- Subagent fan-out for matrix build analysis
- Self-hosted runner debugging
- Secrets/environment management
- Workflow visualization
- Cost/billing analysis
