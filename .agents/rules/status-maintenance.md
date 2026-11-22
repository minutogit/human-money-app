---
name: status-maintenance
description: Rule for maintaining STATUS.md in the project root.
activation: always
---

# STATUS.md Maintenance

Keep [`STATUS.md`](../../STATUS.md) in the project root up to date whenever features finish, priorities shift, or cross-repo dependencies change. Skip trivial wording/formatting edits.

## Format

YAML frontmatter (`project`, `version`, `phase`, `health`, `last_updated`, `blocks`, `blocked_by`, `priority_tasks` with `status: "<open|in-progress|completed|closed|blocked>"`, `priority: "<critical|high|medium|low>"`), followed by concise **Current Focus**, **Architecture**, **Known Issues**, **Recent Milestones**, and **Next Milestones** sections.

## Cross-Project Task IDs
- `CORE-NNN` → `human-money-core`
- `PROTO-NNN` → `humoco-proto`
- `SYNC-NNN` → `humoco-sync`
- `WOT-NNN` → `humoco-web-of-trust`
- `APP-NNN` → `human-money-app`
- `NODE-NNN` → `human-money-node` *(frozen via `NODE-004`)*

## Rules
- Always update `last_updated` (`YYYY-MM-DD`) and keep body consistent with frontmatter.
- Write `STATUS.md` in idiomatic **English**.
- Meta coordination (`../MASTER_OVERVIEW.md` and `../TODO.md`) aggregates from this file via `/update-status`.
