---
title: "feat: GitHub Actions Debugging Skill"
type: feat
date: 2026-02-26
deepened: 2026-02-26
brainstorm: docs/brainstorms/2026-02-26-gha-debugging-skill-brainstorm.md
---

# feat: GitHub Actions Debugging Skill (`/gha`)

## Enhancement Summary

**Deepened on:** 2026-02-26
**Research agents used:** best-practices-researcher, framework-docs-researcher (gh CLI), architecture-strategist, security-sentinel, code-simplicity-reviewer, agent-native-reviewer, skill-authoring-patterns

### Key Improvements

1. **Simplified to two modes** — Create mode removed (Claude can generate workflows without a dedicated mode). Fix mode merged into Debug as a natural flow.
2. **Security hardening** — Input validation for run IDs and file paths, path confinement to `.github/workflows/`, read-only constraint on workflow YAML analysis.
3. **Agent-native design** — `--branch` override for CI contexts (detached HEAD), structured summary output for composability.
4. **Skill authoring alignment** — Uses `$ARGUMENTS` pattern, imperative voice, target under 300 lines.
5. **Two-window log truncation** — First 50 lines (setup context) + last 450 lines (error tail) instead of blind `tail -500`.
6. **Current GHA best practices** — Pinned SHAs for common actions (2025-2026), Node 20 deprecation awareness, `ubuntu-24.04` over `ubuntu-latest` guidance.

### New Considerations Discovered

- `--log-failed` returns empty for success/cancelled runs (not an error — by design). Skill must handle this gracefully.
- `gh run view` exit code is 1 for failed runs — don't treat as a gh CLI error.
- Shell metacharacter injection risk if run IDs or file paths are passed unsanitized to `gh` commands.
- Detached HEAD (common in CI) breaks `git branch --show-current` — need `--branch` fallback.

---

## Overview

A global Claude Code skill installed at `~/.claude/commands/gha.md` that debugs failing GitHub Actions runs and audits existing workflow YAML. Uses the `gh` CLI for all GitHub API access. Invoked as `/gha` with smart defaults and explicit overrides.

## Problem Statement

Debugging GitHub Actions failures is a context-switching chore: open the GitHub UI, find the failed run, expand the right job, scroll through logs, cross-reference with the workflow YAML, figure out what went wrong, switch back to the editor to fix it. This skill keeps the developer in their terminal, using Claude to do the log parsing, root cause analysis, and fix suggestion.

## Proposed Solution

A single skill file with two modes routed by argument parsing:

| Mode | Trigger | Behavior |
|------|---------|----------|
| **Debug** (default) | `/gha` or `/gha <run-id>` | Fetch failed logs, analyze, read workflow YAML, suggest fixes |
| **Fix** | `/gha fix <file>` | Read workflow YAML, audit for issues, suggest fixes |

All modes **suggest fixes with confirmation** before editing files. No auto-apply.

### Research Insight: Why Two Modes, Not Three

The simplicity review found that Create mode adds significant plan complexity for a feature Claude already handles well: "Just ask Claude to create a workflow" works without a dedicated mode. If a user runs `/gha` and has no workflows, the skill can suggest creating one inline. This cuts ~30% of the skill file.

## Technical Approach

### File Structure

Single file, target under 300 lines:

```
~/.claude/commands/gha.md
```

### Research Insight: Skill Authoring Patterns

Based on analysis of existing Claude Code skills (`create-agent-skills`, `ui-skills`, `nightshift`, `beads`):

- Use `$ARGUMENTS` placeholder for argument table (Claude Code injects user args here)
- Write in **imperative voice** ("Fetch the failed logs" not "You should fetch the failed logs")
- Pin exact `gh` commands — don't describe intent, show the command
- Keep under 500 lines total (target 300 for v1)
- Consider `allowed-tools` in frontmatter if the skill should be restricted

**Skill file structure:**
```markdown
---
description: Debug failing GitHub Actions runs and audit workflow YAML
allowed-tools: Bash, Read, Edit, Glob, Grep, AskUserQuestion
---

# /gha — GitHub Actions Debugger

Arguments: $ARGUMENTS

## Argument Routing
[decision tree]

## Prerequisites
[checks]

## Debug Mode
[steps]

## Fix Mode
[steps]
```

### Argument Routing

Decision tree for `$ARGUMENTS`:

```
(empty)                 → Debug mode (auto-detect latest failure)
<number>                → Debug mode (explicit run ID)
fix <path>              → Fix mode (full path or bare name)
fix                     → Fix mode (list workflows, ask which)
help                    → Show usage summary
<other>                 → Show usage hint
```

### Prerequisites Check

Before any mode-specific logic, verify:

1. **`gh` in PATH** — if missing: "Install the GitHub CLI: https://cli.github.com"
2. **`gh auth status`** — if not authenticated: "Run `gh auth login` to authenticate"
3. **Git repo with GitHub remote** — if missing: "This command requires a GitHub repository"

Surface the first failing check with an actionable message. Do not continue.

### Research Insight: Prerequisites Implementation

The architecture review recommends keeping this lightweight. Claude doesn't need a formal check sequence — just run `gh auth status` and if it fails, the error message is self-explanatory. The skill prompt can say: "Run `gh auth status`. If it fails, tell the user what to do and stop."

### Debug Mode

**Step 1: Find the failed run**

Auto-detect (no args):
```bash
gh run list --status=failure --branch="$(git branch --show-current)" --limit=1 --json databaseId,displayTitle,workflowName,headBranch,conclusion,createdAt
```

If no failures on current branch, fall back to all branches:
```bash
gh run list --status=failure --limit=5 --json databaseId,displayTitle,workflowName,headBranch,conclusion,createdAt
```

If no failures at all, show recent runs for context and offer Fix mode.

Explicit run ID: validate with `gh run view <id> --json status,conclusion`. If the run is not `failure`, show the actual status and offer appropriate actions:
- `in_progress` → "Run is still in progress. Watch with `gh run watch <id>`"
- `success` → "Run passed. No failures to debug."
- `cancelled` → "Run was cancelled. Showing available logs."

#### Research Insight: Detached HEAD and `--branch` Override

`git branch --show-current` returns empty in detached HEAD state (common in CI, after `git checkout <sha>`, or during rebase). The skill must handle this:

- If `git branch --show-current` returns empty, skip branch filtering and fall back to all-branches query
- For agent-native usage in CI, accept an explicit branch: `/gha --branch=main` or `/gha <run-id>` (run IDs bypass branch detection entirely)

#### Research Insight: gh CLI Exit Codes

`gh run view` returns exit code **1** for failed runs. This is by design, not an error. The skill must not treat a non-zero exit from `gh run view` as a gh CLI failure. Check `conclusion` from JSON output instead.

**Step 2: Fetch failed logs**

Two-window truncation (preserves both setup context and error tail):
```bash
gh run view <run-id> --log-failed 2>&1 | { head -50; echo "--- [truncated] ---"; tail -450; }
```

This captures:
- **First 50 lines**: Job setup, environment info, early steps that provide context
- **Last 450 lines**: The actual error output, stack traces, failure messages

If `--log-failed` returns empty (run succeeded or was cancelled), note it: "No failed step logs available for this run."

For runs with multiple failed jobs, also fetch job metadata:
```bash
gh run view <run-id> --json jobs --jq '.jobs[] | select(.conclusion == "failure") | {name, conclusion}'
```

#### Research Insight: `--log-failed` Internals

From gh CLI source analysis: `--log-failed` downloads the full log ZIP from the API, then filters steps where `IsFailureState` is true (covers `failure`, `action_required`, `startup_failure`, `timed_out`). Key behaviors:
- Returns empty for success/cancelled runs — not an error
- The log ZIP can be large (10MB+) for matrix builds — gh handles the download/extraction
- Rate limit: log downloads count as API requests (5,000/hour for authenticated users)
- No `--json` equivalent for logs — text output only

**Step 3: Locate the workflow YAML**

Read the local `.github/workflows/` file matching the workflow name from the run metadata.

```bash
gh run view <run-id> --json headSha,workflowName
```

If the run's `headSha` differs from current HEAD, mention it. But always read the local file — the user wants to fix what's on disk.

**Step 4: Analyze and suggest fixes**

Read referenced source files if the failure is in application code (test files, build configs). Present:

1. Root cause explanation — what failed and why
2. The specific error from the logs (quote the relevant lines)
3. The relevant section of the workflow YAML (or source file)
4. Concrete fix suggestion
5. Ask for confirmation before applying any edits

Let Claude's natural analysis handle categorization — don't prescribe a rigid taxonomy in the skill prompt. The error categories table from the original plan is useful context for the *plan reader* but should not be baked into the skill as a lookup table.

### Fix Mode

**Step 1: Locate the workflow file**

Accept full path (`.github/workflows/ci.yml`) or bare name (`ci` → resolved to `.github/workflows/ci.yml` or `.yaml`). If bare name matches multiple files, list matches and ask user to pick.

If file not found, list available workflow files and ask which to fix.

#### Research Insight: Path Confinement

Only read files within `.github/workflows/`. If a user passes a path outside this directory, note it's not a workflow file. Do not use unsanitized user input in shell commands — pass file paths as arguments to `Read`, not interpolated into bash strings.

**Step 2: Read and audit**

Read the YAML and check for common issues. The skill prompt should list these as examples, not an exhaustive checklist — Claude will naturally spot other problems:

- **Unpinned actions**: Using `@v3` tag instead of SHA (supply chain risk, especially post-tj-actions incident March 2025)
- **Deprecated syntax**: `::set-output`, `::save-state` → use `$GITHUB_OUTPUT`, `$GITHUB_STATE`
- **Deprecated runtimes**: Node 16 actions (removed), Node 20 (deprecated June 2025, removal June 2026)
- **Missing permissions block**: No `permissions:` → add least-privilege
- **Missing concurrency**: No `concurrency:` for PR workflows → cancel stale runs
- **Missing timeout**: No `timeout-minutes:` → prevent runaway jobs
- **Runner pinning**: `ubuntu-latest` currently resolves to `ubuntu-24.04` — note trade-off (latest vs reproducible)

**Step 3: Suggest fixes**

Present issues found with explanations. Show proposed changes. Ask for confirmation before applying edits.

### Research Insight: Current Action SHAs (2025-2026)

For SHA-pinning recommendations, use these current versions:

| Action | Version | SHA |
|--------|---------|-----|
| `actions/checkout` | v6.0.2 | `de0fac2e` (first 8 chars — use full SHA in actual skill) |
| `actions/setup-node` | v6.2.0 | `6044e13b` |
| `actions/cache` | v5.0.3 | `cdf6c1fa` |
| `actions/setup-python` | v6.2.0 | `a309ff8b` |

**Important**: These SHAs will go stale. The skill should instruct Claude to verify current SHAs via `gh api repos/actions/checkout/releases/latest` or similar when recommending pins, rather than hardcoding SHAs in the skill file.

### Security Considerations

From the security review (7 findings, 3 high/medium priority):

1. **Input validation** (HIGH): Run IDs must be numeric. File paths must resolve within `.github/workflows/`. Never interpolate user input directly into shell commands — use `gh run view "$RUN_ID"` with proper quoting.

2. **Sensitive data in logs** (MEDIUM): GHA logs may contain leaked secrets, tokens, or credentials. The skill should not echo raw log content back to the user in full — the two-window truncation helps, but also instruct Claude to watch for and redact obvious secrets (e.g., patterns matching `ghp_*`, `ghs_*`, `AKIA*`).

3. **Read-only constraint** (MEDIUM): Fix mode should only suggest edits to workflow YAML files. The skill should never write to files outside `.github/workflows/` in Fix mode. Debug mode may suggest edits to source files (test files, configs) — this is expected.

4. **YAML generation safety** (LOW): When suggesting workflow YAML changes, avoid patterns vulnerable to expression injection (e.g., `${{ github.event.issue.title }}` in `run:` blocks). Prefer environment variables: `env: { TITLE: "${{ github.event.issue.title }}" }` then `run: echo "$TITLE"`.

## Acceptance Criteria

- [x] Skill file created at `~/.claude/commands/gha.md`
- [x] Skill uses `$ARGUMENTS` pattern and imperative voice
- [x] Under 300 lines (stretch: under 250) — **177 lines**
- [x] `/gha` with no args auto-detects latest failed run on current branch
- [x] `/gha <run-id>` debugs a specific run
- [x] `/gha fix <file>` audits and suggests fixes for existing workflow YAML
- [x] Prerequisites check (gh installed, authenticated, git repo) with actionable errors
- [x] Non-failed run IDs handled gracefully (show status, not empty output)
- [x] Empty `--log-failed` output handled (success/cancelled runs)
- [x] Two-window log truncation (head 50 + tail 450) to prevent context overflow
- [x] All suggestions require confirmation — no auto-apply
- [x] Bare workflow names resolved in fix mode (e.g., `ci` → `.github/workflows/ci.yml`)
- [x] Run IDs validated as numeric before passing to shell commands
- [x] File paths confined to `.github/workflows/`
- [x] Detached HEAD handled gracefully (skip branch filter)

## Decisions Resolved (from brainstorm open questions)

| Question | Decision | Rationale |
|----------|----------|-----------|
| Auto-apply vs. suggest? | **Suggest with confirmation** in all modes | Consistent behavior, no surprise edits, aligns with Claude Code conventions |
| How deep should log analysis go? | **Two-window: head 50 + tail 450** of `--log-failed` | Preserves setup context + error output, prevents context overflow |
| Self-hosted runners? | **No** in v1 | Stick to GitHub-hosted, simpler scope |
| Local file vs. commit SHA? | **Read local, note SHA mismatch** | Pragmatic — user wants to fix the current file |
| Branch-aware auto-detect? | **Yes** — filter by current branch first, fall back to all | Reduces noise in active repos |
| Create mode? | **Removed** — Claude handles workflow authoring without a dedicated mode | Simplicity review: cuts 30% of skill, feature works fine as "just ask Claude" |
| Error categorization table? | **Not in skill** — useful plan context but don't prescribe taxonomy in prompt | Simplicity review: overly prescriptive prompts produce worse results than clear intent + constraints |
| Hardcoded SHAs? | **No** — skill instructs Claude to look up current SHAs at runtime | SHAs go stale; runtime lookup via `gh api` is more reliable |

## Out of Scope (v1)

- Subagent fan-out for matrix build analysis
- Self-hosted runner debugging
- Secrets/environment management
- Workflow visualization
- Cost/billing analysis
- GitHub Enterprise Server-specific logic
- Fork/upstream disambiguation
- Retry attempt analysis (`--attempt N`)
- Bundled reference files or scripts
- Create mode (Claude can generate workflows without a dedicated skill mode)
- `--auto` / `--dry-run` flags for fully autonomous operation (v2 consideration)

## References

- Brainstorm: `docs/brainstorms/2026-02-26-gha-debugging-skill-brainstorm.md`
- GitHub CLI docs: https://cli.github.com/manual/
- GitHub Actions workflow syntax: https://docs.github.com/en/actions/using-workflows/workflow-syntax-for-github-actions
- GitHub Actions security hardening: https://docs.github.com/en/actions/security-for-github-actions/security-hardening-your-deployments
- Skill format: `~/.claude/commands/` with markdown body, `$ARGUMENTS` for user args
- gh CLI source (log download): `pkg/cmd/run/view/view.go` — `attachRunLog()` and `IsFailureState()`
- tj-actions supply chain attack (March 2025): Rationale for SHA pinning over tag pinning
