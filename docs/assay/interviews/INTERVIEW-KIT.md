# Assay interview kit: 10 conversations in 2 weeks

**Goal:** find out whether people who let AI agents act on their computer have a real problem Assay solves, and whether they would pay. Run alongside the A1-01 build. Finish by **4 November 2026**.

**The rule for every conversation:** ask about what already happened, never about what someone would do. "Would you use…" gets polite yeses that mean nothing. "Tell me about the last time…" gets the truth.

**Do not mention Assay until question 6.** If you describe it first, every answer bends towards it.

## 1. Who counts

Someone who in the last month let an AI agent do things on their own computer or accounts, not just chat. For example:

- Claude Code, Cursor, Windsurf or Copilot agent mode editing files or running commands;
- Claude Desktop or ChatGPT with tools connected;
- OpenClaw, Hermes, or any always-on agent.

Aim for a mix: at least 3 who are not software developers.

**Do not recruit:** people at your employer, people who would say yes to please you, or anyone who has seen the Assay page.

## 2. Where to find them

| Place | How |
|---|---|
| r/ClaudeAI, r/cursor, r/LocalLLaMA | Find posts where someone complains that an agent deleted, overwrote or ran something. Message the poster |
| OpenClaw and Hermes Discords, in their help channels | The same: people asking for help after something went wrong |
| Hacker News | Comment threads on agent incidents |
| X / LinkedIn | People posting about their agent setups |
| Your own network | Friends who build with agents. At most 3 of the 10 |

## 3. Outreach message (adapt the first line to their post)

> Hi, I saw your post about [what happened with their agent]. I'm researching how people deal with AI agents doing things on their computers. Not selling anything. Could I ask you 5 questions on a 20-minute call this week? Happy to share what I learn from everyone.

## 4. The conversation (20 minutes)

1. **"Which AI agents do you let act on your computer or accounts? What do they do for you?"**
   Note each agent and what it touches: files, shell, email, money, accounts.
2. **"Tell me about the last time an agent did something you didn't want, or didn't expect."**
   Follow up with: When was it? What exactly happened? How did you find out? What did it cost you: time, data, money, embarrassment?
   *If they have no story, ask: "What's the worst thing it could do with the access it has?" Then go to question 5.*
3. **"What did you do about it afterwards?"**
   Listen for the workarounds people already use: approval mode, sandboxes, git, backups, avoiding certain tasks, stopping use altogether.
4. **"How much time or money do those safeguards cost you now?"**
   Anything they already spend is the real price signal.
5. **"When an agent says it did something, how do you check it actually did it?"**
   Listen for: they don't check, they check by hand, or they have caught it lying.
6. **Only now, describe Assay in one sentence:** *"A small layer every agent on your computer goes through: it asks you before risky actions, checks what actually happened, and keeps a receipt you can't tamper with."*
   Then ask: "What would you need to see before you trusted it?"
7. **The commitment question:** "I'm building a first version for Mac in the next few weeks. Want to be one of the first 20 people to test it?" If yes: "Would you pay $10 a month for it if it worked?" Write down their exact words.

Close with: "Who else do you know who'd have a story like this?"

## 5. What the results must show (decided now, before the first call)

| Signal | Count needed out of 10 | If it falls short |
|---|---|---|
| Has a real story: an agent did something unwanted, with a cost | **3 or more** | The pain is not felt yet. Keep building the slice, but rethink who to sell to before Gate 2 |
| Already spends time or money on safeguards | **3 or more** | Weak willingness to pay. Look again at price and who pays |
| Says yes to testing | **5 or more** | Weak pull. Find out why from the people who said no |
| Says yes to $10 a month in their own words | **2 or more** | Do not plan revenue yet |

Record the results in `tracker.csv`, one row per conversation, on the same day.
