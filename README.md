# agent-fork

**OS-level Copy-on-Write time-travel debugger and parallel trajectory forker for AI coding agents.**

[![CI](https://github.com/abhinavm-web/agent-fork/actions/workflows/ci.yml/badge.svg)](https://github.com/abhinavm-web/agent-fork/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/abhinavm-web/agent-fork)](https://github.com/abhinavm-web/agent-fork/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](Cargo.toml)

<p align="center">
  <img src="assets/demo.svg" width="860" alt="agent-fork terminal demo">
</p>

---

## Overview

Modern coding agents execute 30–100+ step trajectories across file edits, shell commands, package installs, database migrations, and subagent processes. When an agent goes off the rails at Step 42, you cannot cleanly step back to Step 18, tweak a prompt or tool output, and branch execution.

Built-in agent rewinds (such as Claude Code's `/rewind`) only track files touched by the agent's **internal file-edit tool**. The moment an agent runs `bash` (`npm install`, `rm`, Python scripts, SQLite/Postgres migrations, or subagent workers), internal checkpointing goes blind—leaving mutated filesystems, half-broken dependencies, and dirty state behind.

`agent-fork` wraps **any** CLI coding agent (Claude Code, Codex, Gemini CLI, OpenCode, or custom scripts) in a **content-addressed Copy-on-Write (CoW) workspace sandbox** paired with a **local HTTP/SSE LLM & MCP cassette proxy**:

- **Zero-overhead CoW snapshots**: Captures full workspace trees (`blake3`-indexed content-addressed object store with hardlink/reflink materialization) on every turn boundary or explicit checkpoint, including shell side effects, generated binaries, and local SQLite files.
- **Deterministic LLM/MCP Cassette Proxy**: Intercepts `ANTHROPIC_BASE_URL` and `OPENAI_BASE_URL` traffic, recording request/response turns alongside each filesystem checkpoint so historical steps can be inspected, edited, and replayed.
- **Parallel Trajectory Forking**: Rewind the workspace to any historical step or fork multiple isolated branch worktrees side-by-side from a shared checkpoint, compare diffs, and promote the winning trajectory back into the primary workspace.

```text
  Agent Process (claude / codex / gemini / custom script)
         |                              |
         v                              v
  +--------------+              +-------------------+
  | Local HTTP   |<------------>| CoW Filesystem    |
  | Cassette     |              | Snapshot Engine   |
  | Proxy (Axum) |              | (blake3 + CoW)    |
  +------+-------+              +---------+---------+
         |                                |
         +--------------+-----------------+
                        v
            +-----------------------+
            | Session Timeline CLI  |
            | step_0018 --+-> run-a |
            |             +-> run-b |
            +-----------------------+
```

## Installation & 30-Second Showcase

Install prebuilt binaries with one command, download directly from [GitHub Releases](https://github.com/abhinavm-web/agent-fork/releases/latest), or build from source:

```bash
# macOS (Apple Silicon) & Linux (x86_64)
curl -fsSL https://raw.githubusercontent.com/abhinavm-web/agent-fork/main/install.sh | sh
```

```powershell
# Windows (PowerShell)
irm https://raw.githubusercontent.com/abhinavm-web/agent-fork/main/install.ps1 | iex
```

Or run from source:

```bash
git clone https://github.com/abhinavm-web/agent-fork.git
cd agent-fork

# Run the self-contained Copy-on-Write rewind & trajectory forking showcase
cargo run --example showcase

# Or install the CLI binary globally
cargo install --path .
```

## Usage

### 0. Install zero-touch per-tool-call hooks for Claude Code

```bash
# Registers a PostToolUse hook in .claude/settings.local.json so every Bash/Edit/Write
# tool call inside an interactive Claude Code session automatically checkpoints CoW state
agent-fork hook install
agent-fork hook status
```

### 1. Run an agent inside the CoW sandbox + cassette proxy

```bash
# Automatically snapshots pre/post workspace state and routes API calls through the local cassette proxy
agent-fork run -m "Refactor auth to JWT" -- claude "Refactor auth to JWT and migrate local sqlite db"

# Record filesystem mutations only (disable HTTP proxy interception)
agent-fork run --no-proxy -- pytest -q
```

### 2. Inspect the timeline and diffs

```bash
# View recorded steps, modification counts, and linked cassette turns
agent-fork log

# Output timeline as structured JSON
agent-fork log --json

# Compare any two steps or compare a step against the current working tree
agent-fork diff step_0001 step_0002
agent-fork diff step_0001 --stat
```

### 3. Rewind or fork parallel trajectories

```bash
# Create an explicit manual checkpoint at any point
agent-fork checkpoint -m "Before database migration"

# Rewind workspace files (and clean untracked build artifacts) back to step 1
agent-fork rewind step_0001 --force

# Fork two isolated branch worktrees from step 1 to test competing approaches in parallel
agent-fork fork step_0001 --variant session-cookies --variant jwt-rotation

# List active branches and their step pointers
agent-fork branches

# Promote the winning variant branch back into the main workspace
agent-fork promote jwt-rotation --force
```

### 4. Inspect and edit recorded LLM/MCP cassette turns

```bash
# List recorded API turns
agent-fork cassette list

# Inspect full request and response payload for a turn
agent-fork cassette show <cassette-id>

# Modify a recorded prompt or tool output for deterministic replay
agent-fork cassette edit <cassette-id> --prompt "Use Ed25519 keys instead of RS256"
```

## Architecture

```text
src/
├── cli.rs              # Clap CLI definition & subcommands
├── error.rs            # Typed error hierarchy (ForkError)
├── cow/
│   ├── engine.rs       # Workspace snapshot, restore, and branch materialization
│   ├── storage.rs      # Content-addressed blake3 blob store (.agent-fork/objects)
│   ├── manifest.rs     # Per-step file metadata & hash manifests
│   ├── diff.rs         # Unified diff & line-stat computation across steps
│   └── ignore.rs       # .gitignore and sandbox exclusion filters
├── cassette/
│   ├── proxy.rs        # Axum HTTP proxy intercepting Anthropic & OpenAI turns
│   ├── store.rs        # Cassette persistence (.agent-fork/cassettes)
│   ├── edit.rs         # Turn prompt/response mutation
│   └── signature.rs    # Deterministic request fingerprinting
├── process/
│   └── runner.rs       # Child process spawning with injected proxy env vars
└── session/
    ├── timeline.rs     # Step history & pointer state machine
    └── branch.rs       # Isolated branch worktree management & promotion
```

## Development & Verification

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

## Related Tools (Agent Systems Toolkit)

| Repository | Focus |
| :--- | :--- |
| **[agent-fork](https://github.com/abhinavm-web/agent-fork)** | OS-level Copy-on-Write time-travel debugger & parallel trajectory forker for coding agents |
| **[ctx-firewall](https://github.com/abhinavm-web/ctx-firewall)** | Active LLM context proxy & Ratatui flamegraph profiler that prunes context rot without breaking KV-cache |
| **[diffmem](https://github.com/abhinavm-web/diffmem)** | Zero-touch "Diff-of-Diffs" episodic failure memory across Claude Code, Cursor, and AGENTS.md |
| **[reddiff](https://github.com/abhinavm-web/reddiff)** | Zero-prose adversarial diff breaker: runnable crash PoC or silence |
| **[jitpipe](https://github.com/abhinavm-web/jitpipe)** | JIT-compiled semantic Unix pipes (`sgrep`, `sawk`, `sjq`) streaming at native Rust speed |

## License

[MIT](LICENSE)
