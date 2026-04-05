---
name: status-maintenance
description: Rule for maintaining the project STATUS.md file.
activation: always
---

# STATUS.md Maintenance

You are responsible for keeping `STATUS.md` in the project root up to date.

## When to Update

Update `STATUS.md` whenever:
- A significant feature is completed or a milestone is reached
- A new bug or blocker is discovered
- Task priorities change
- The project phase changes (e.g., from "planning" to "active-development")
- Dependencies on other HuMoCo projects change

## Format

The file uses YAML frontmatter with these fields:

```yaml
---
project: <project-name>
version: "<semver>"
phase: "<planning|research|active-development|alpha|beta|stable>"
health: "<green|yellow|red>"
last_updated: "<YYYY-MM-DD>"
blocks: []          # Task IDs in OTHER projects that this project blocks
blocked_by: []      # Task IDs in OTHER projects that block this project
priority_tasks:
  - id: "<PROJECT-NNN>"
    title: "<short title>"
    status: "<open|in-progress|done|blocked>"
    priority: "<critical|high|medium|low>"
    depends_on: ["<OTHER-NNN>"]
    description: "<one-line description>"
---
```

Below the frontmatter, maintain free-form Markdown sections:
- **Current Focus**: What is being actively worked on
- **Known Issues**: Bugs, blockers, technical debt
- **Recent Milestones**: What was recently completed (checkbox list)
- **Next Milestones**: What comes next (checkbox list)

## Cross-Project Task IDs

Use these prefixes for task IDs across the ecosystem:
- `CORE-NNN` → human-money-core
- `APP-NNN` → human-money-app
- `L2-NNN` → humoco-layer2-planing
- `WOT-NNN` → humoco-web-of-trust

## Important

- Always update `last_updated` when making changes
- Keep the language **English**
- The overview coordinator agent in `humoco-overview` reads this file — keep it accurate
