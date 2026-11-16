# agent-fork

**OS-level Copy-on-Write time-travel debugger and parallel trajectory forker for AI coding agents.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](#roadmap--mvp-milestones)
[![Architecture: MVP](https://img.shields.io/badge/Architecture-MVP-orange.svg)](#roadmap--mvp-milestones)

---

## The Problem

Modern coding agents execute 30–100+ step trajectories across file edits, shell commands, package installs, database migrations, and subagents. When an agent goes off the rails at Step 42, you cannot cleanly step back to Step 18, tweak a prompt or mock a tool output, and branch execution.

Existing built-in rewind features (such as Claude Code's `/rewind`) only snapshot files modified through the agent's **internal file-edit tool**. The moment an agent runs `bash` (`npm install`, `rm`, Python scripts, SQLite/Postgres migrations, or subagent workers), internal checkpointing goes blind—leaving mutated filesystems, half-broken dependencies, and dirty state behind.

## The Unclaimed Wedge: How It Works

`agent-fork` wraps **any** CLI coding agent (Claude Code, Codex, Gemini CLI, OpenCode, or custom scripts) inside an **OS-level Copy-on-Write (CoW) workspace sandbox** paired with a **local LLM/MCP cassette proxy**:

- **Zero-overhead CoW snapshots**: Uses APFS `clonefile` (macOS), OverlayFS / Btrfs reflinks (Linux), or git-tree/DevDrive clones (Windows) to snapshot the *entire* workspace—including `node_modules`, local SQLite files, and generated artifacts—on every tool call or turn boundary.
- **Deterministic HTTP/MCP Cassette**: Records every LLM response and MCP tool payload alongside each filesystem checkpoint so past steps can be replayed without re-paying API costs.
- **Branchable Futures**: Jump back to any step, edit the instruction or mock a tool response, and **fork two parallel agent trajectories side-by-side** from the exact same disk + context state.

```text
  Agent (Claude / Codex / Gemini / Custom)
         │                        │
         ▼                        ▼
  ┌──────────────┐        ┌───────────────────┐
  │ LLM/MCP      │◄──────►│ CoW Filesystem    │
  │ Cassette     │        │ Snapshot Engine   │
  │ Proxy        │        │ (APFS / OverlayFS)│
  └──────┬───────┘        └─────────┬─────────┘
         │                          │
         └──────────┬───────────────┘
                    ▼
        ┌───────────────────────┐
        │  Timeline & Fork UI   │
        │  Step 18 ─┬─► Run A   │
        │           └─► Run B   │
        └───────────────────────┘
```

### Target CLI Experience (`< 60 second aha moment`)

> Note: Below is the target MVP CLI specification currently under construction.

```bash
# 1. Wrap any coding agent session with full CoW + cassette recording
agent-fork run -- claude "Refactor auth to JWT and migrate local sqlite db"

# 2. Agent breaks migrations at Step 27? Inspect timeline and rewind disk + state
agent-fork log
#  step 14  [edit] src/auth.ts          (+42 -11)   0.4ms snapshot
#  step 18  [bash] npx prisma migrate   (exit 0)    1.1ms snapshot
#  step 27  [bash] npm test             (exit 1)    0.9ms snapshot  <-- went wrong

# 3. Fork two competing approaches from Step 18 in parallel isolated snapshots
agent-fork fork 18 --variant "Use session cookies instead" --variant "Fix JWT refresh rotation"

# 4. Compare diffs & test results side-by-side, then promote the winner
agent-fork promote run-b
```

## Roadmap & MVP Milestones

- [ ] **Milestone 1 — CoW Snapshot Engine**: Cross-platform snapshot driver (macOS APFS `clonefile`, Linux OverlayFS/Btrfs, Windows git-object/junction fallback) with sub-10ms checkpointing.
- [ ] **Milestone 2 — Agent Process & Proxy Wrapper**: `agent-fork run -- <cmd>` launcher setting `ANTHROPIC_BASE_URL` / `OPENAI_BASE_URL` to record turn-by-turn LLM + tool cassettes linked to snapshot IDs.
- [ ] **Milestone 3 — Timeline & Fork CLI**: `log`, `diff`, `rewind <step>`, `fork <step>`, and `promote <branch>` commands.
- [ ] **Milestone 4 — Local Web/TUI Trajectory Viewer**: Interactive timeline scrub bar showing context diff + filesystem diff + bash exit codes at every step.

## License

[MIT](LICENSE)
