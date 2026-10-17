# Agent Instructions

## Agent Continuity Protocol

This project keeps its working state in the repository so any coding agent (Codex, Claude,
Goose/Nightwire, local Qwen, ...) can pick it up without the previous conversation.

At the start of substantial work, read:

1. `AGENTS.md`
2. `docs/agent/HANDOFF.md`
3. `docs/agent/DECISIONS.md`
4. `docs/agent/BLOCKERS.md`
5. `docs/wiki/README.md`, then only the wiki pages relevant to the task
6. Recent entries under `docs/agent/logs/`, and the newest records under `docs/agent/jobs/`, when useful

Verify `HANDOFF.md` against the repository before relying on it: Git state and source code win
when documentation is stale.

During work:

- Record durable project, product or architecture decisions in `docs/agent/DECISIONS.md`, and
  only ones a human made or approved.
- Record unresolved issues that need human input in `docs/agent/BLOCKERS.md` as `## OPEN — <title>`.
- Never invent a decision to unblock implementation, and never mark a blocker RESOLVED yourself.

Before ending substantial work (including partial or failed work):

1. Update `docs/agent/HANDOFF.md` in place for an agent with zero prior context.
2. Append a dated entry to `docs/agent/logs/YYYY-MM-DD.md` (append only, no transcripts).
3. Update `DECISIONS.md` and `BLOCKERS.md` when they changed.
4. Put knowledge an agent will still need in a month in `docs/wiki/` (update the closest
   existing page and the `docs/wiki/README.md` index). Next-session state goes in the handoff,
   what happened goes in the daily log, and questions for a human go in `BLOCKERS.md`.

Harness jobs write their handoff and log entry to their own `docs/agent/jobs/<date>-<job>.md` instead
of editing `HANDOFF.md` and the daily log, so pull requests from different jobs never conflict. Read the
newest job records as the continuation of `HANDOFF.md`.

Never write secrets, API keys, tokens, passwords, private keys, cookies, credentials or
environment variable values into any of these files. Do not create agent-specific handoff
files such as `claude-handoff.md` or `codex-handoff.md`; `docs/agent/HANDOFF.md` is the only one.
