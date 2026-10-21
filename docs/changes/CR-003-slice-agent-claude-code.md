# CR-003: SLICE-01's agent is Claude Code, not Claude Desktop

| | |
| --- | --- |
| Raised | 21 October 2026, by the owner |
| Kind | Change of scope inside story A1-01 (the agent it serves) |
| Decision | Taken by the owner in the session of 21 October 2026, recorded below in his words. Becomes binding when he approves the pull request that carries this file |

## The owner's decision, in his words

> Decision: SLICE-01's agent changes from Claude Desktop to Claude Code.
> - Assay installs into a project-scoped ~/AssayLab/.mcp.json for a Claude Code session opened on ~/AssayLab. It never touches claude_desktop_config.json or ~/.claude.json.
> - Check 18 becomes: "assay uninstall removes exactly what install added; ~/AssayLab/.mcp.json is restored byte for byte, or removed if install created it."
> - Measure Claude Code's tool-call time limit the same way, with a probe in ~/AssayLab/.mcp.json, without quitting this app.
> - Claude Desktop becomes a later story.

## Why

Claude Desktop's app rewrote its own settings file and dropped the test server the crew had added. See `docs/work/A1-01/claude-desktop-findings.md`.

## What changes

1. The work order for A1-01: the agent, check 18, step 8, what the owner needs at the Mac, and the time-limit question.
2. The backlog: A1-01's acceptance text and epic A1's outcome name Claude Code instead of Claude Desktop. The status is unchanged.
3. `CLAUDE.md`: the line on the current work says "a Claude Code session" instead of "Claude Desktop".

## What it displaces (BR-17)

Nothing of equal size is added. The same slice serves a different agent. Claude Desktop support moves to a later story that has not been written yet.
