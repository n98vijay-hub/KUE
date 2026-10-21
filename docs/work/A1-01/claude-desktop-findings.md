# Why SLICE-01 moved from Claude Desktop to Claude Code

| | |
| --- | --- |
| Date | 21 October 2026 |
| Written by | The lead, on the owner's instruction |
| Decision | The owner's, the same day: the agent for SLICE-01 is a Claude Code session opened on `~/AssayLab`. Claude Desktop becomes a later story. Recorded in CR-003 |

## What was tried

To measure Claude Desktop's own time limit for a tool call, the crew added one test server (`assay-timeout-probe`, a small program that only waits) to Claude Desktop's settings file, `~/Library/Application Support/Claude/claude_desktop_config.json`. First it copied the original file byte for byte (14,015 bytes) to a backup. The owner then quit Claude, reopened it and asked a new chat to use the test tool.

## What was seen

| Observation | Evidence |
| --- | --- |
| The Claude app's version was 1.11187.4 | The app's `Info.plist`, read |
| The app quit at 16:13:52 and started again at 16:13:57 | The app's own log (`~/Library/Logs/Claude/main.log`), read |
| At that start the app launched five local tool servers (four extensions and one plugin) and never tried the test server | The same log: no line names the test server, and no log file was made for it, as there is for each server the app starts |
| The test server never ran and never received a request | Its own log file was never created |
| The chat answered that it had no tool of that name | The owner's report |
| At 16:16:01 the app rewrote its settings file. The test server's entry was gone. Every other setting had the same name and value as the original; the bytes were arranged differently, at the same size of 14,015 bytes | File time and a comparison of setting names and values, without printing them |
| No log line on this Mac has ever named `claude_desktop_config.json` | A search of all the app's logs |

## What it means

The test cannot tell which of these is true:

1. The app keeps its own copy of these settings and writes it back, dropping entries added while it ran. The test server was added while the app was open, because the crew's session runs inside the same app. A report on GitHub from five days earlier describes exactly this, on the Windows build: [anthropics/claude-ai-mcp#1150](https://github.com/anthropics/claude-ai-mcp/issues/1150).
2. This version no longer starts tool servers from that file. No notice of such a change was found.

Either way, Assay cannot rely on that file in this slice. The app rewrites it on its own, so `assay install` would be undone. Uninstall could not restore the bytes either, because the app may have changed them, and check 18 would fail through no fault of Assay. Whether the app still reads the file was left untested, by the owner's decision.

## What was kept as it is

- Claude Desktop's settings file stays as the app rewrote it. Its contents match the original.
- The original bytes are kept on the owner's Mac in `Claude outputs/claude-desktop-config-backup.json`, which git ignores.

## For the later Claude Desktop story

Before building, find out:

- whether this version starts servers from the settings file when it is edited with the app fully closed;
- when the app writes the file;
- whether extensions are the supported way to add a local server.
