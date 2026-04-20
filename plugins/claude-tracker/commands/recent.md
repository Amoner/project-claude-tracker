---
allowed-tools: Bash(tracker-cli:*)
description: Show your most-recently-active Claude Code projects
---

## Context

- Recent projects (JSON from tracker DB): !`tracker-cli recent --limit 10`

## Your task

Render the JSON above as a single markdown table with columns: **Project**, **Status**, **Last Active**, **Sessions**, **Prompts**. Format `last_active_at` as a short human-readable string (e.g. "3h ago", "2d ago"). Use `effective_status` for Status. Do not add commentary beyond the table.
