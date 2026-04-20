---
allowed-tools: Bash(open:*), Bash(ls:*), Bash(test:*)
description: Open the Claude Tracker desktop dashboard
---

## Context

- Dashboard launch attempt: !`if [ -d "/Applications/Claude Tracker.app" ]; then open "/Applications/Claude Tracker.app" && echo "opened"; else echo "not installed — download DMG from https://github.com/Amoner/project-claude-tracker/releases"; fi`

## Your task

Report the outcome in one short sentence. If "opened", tell the user the dashboard window should now be visible. If "not installed", point them at the Releases page link in the output.
