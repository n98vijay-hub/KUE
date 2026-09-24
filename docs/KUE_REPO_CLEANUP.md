# KUE repository cleanup report

**2026-09-20.** Canonical line: `main` = `kue/runtime-safety` = `0ecd4ab`.
Nothing below has been deleted. This is the evidence for a later decision.

| Branch | HEAD | Unique commits | Worktree | Safe to delete |
|---|---|---|---|---|
| `backup/main-uncommitted-identity-check` | `53bc557` | 1 | — | **NO — 1 commit(s) exist only here** |
| `claude/personal-ai-prototype-continue-47ecde` | `43d7a84` | 0 | — | **yes** — fully contained in the canonical line |
| `kue/calendar-helper` | `e910366` | 1 | — | **NO — 1 commit(s) exist only here** |
| `kue/computer-use-research` | `b893684` | 0 | — | **yes** — fully contained in the canonical line |
| `kue/reasoning-provider-research` | `79155a1` | 0 | — | **yes** — fully contained in the canonical line |
| `kue/runtime-safety` | `0ecd4ab` | 0 | personal-ai-prototype-continue-47ecde | no — checked out in a worktree |
| `kue/security-invariants` | `79155a1` | 0 | — | **yes** — fully contained in the canonical line |
| `kue/speaker-identity-research` | `79155a1` | 0 | — | **yes** — fully contained in the canonical line |
| `kue/speaker-model-decision` | `6b21cf4` | 2 | — | **NO — 2 commit(s) exist only here** |
| `kue/ui-redesign` | `79155a1` | 0 | — | **yes** — fully contained in the canonical line |
| `kue/web-agent-research` | `0d820f2` | 0 | agent-a96bd65a585135644 | no — checked out in a worktree |
| `main` | `0ecd4ab` | 0 | MY PERSONAL AI ASSISTANT | no — the canonical branch |
| `worktree-agent-a26e37e863db25089` | `43d7a84` | 0 | — | **yes** — fully contained in the canonical line |
| `worktree-agent-a39b460e5c097fbab` | `43d7a84` | 0 | — | **yes** — fully contained in the canonical line |
| `worktree-agent-a433e38daf6849351` | `43d7a84` | 0 | — | **yes** — fully contained in the canonical line |
| `worktree-agent-a5f9b171537510c23` | `43d7a84` | 0 | — | **yes** — fully contained in the canonical line |
| `worktree-agent-a96bd65a585135644` | `43d7a84` | 0 | — | **yes** — fully contained in the canonical line |
| `worktree-agent-ac46840817218e6d1` | `43d7a84` | 0 | — | **yes** — fully contained in the canonical line |
| `worktree-agent-adef6371b99879165` | `43d7a84` | 0 | — | **yes** — fully contained in the canonical line |
| `worktree-agent-af209c0ce7a04f4aa` | `43d7a84` | 0 | — | **yes** — fully contained in the canonical line |

## Worktrees

| Path | Branch | Size | Safe to remove |
|---|---|---|---|
| `MY PERSONAL AI ASSISTANT` | `main` | 15G | no — the main checkout |
| `agent-a96bd65a585135644` | `kue/web-agent-research` | 4.1M | yes, after this session — its branch adds nothing to the canonical line |
| `personal-ai-prototype-continue-47ecde` | `kue/runtime-safety` | 11G | yes, after this session — its branch adds nothing to the canonical line |

## Reading this report

- Three branches carry work that is **not** in the canonical line: `backup/main-uncommitted-identity-check` (1), `kue/calendar-helper` (1, the WIP calendar helper), `kue/speaker-model-decision` (2, older drafts of the model decision). Deleting any of them loses that work.
- The eight `worktree-agent-*` branches and four empty `kue/*` agent branches sit on the pre-rename Lantern commit or on the canonical line and hold nothing of their own.
- Deleting branches and worktrees is on the owner's stop list. Nothing here has been deleted.
