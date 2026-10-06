#!/usr/bin/env python3
"""Ledger rules for KUE.

Checks that a pull request moves stories in docs/product/backlog.json only in
the ways the change-control procedure allows (docs/process/CHANGE-CONTROL.md).

The point of this check: the files that carry the owner's decisions are owned
by him in .github/CODEOWNERS, so GitHub will not merge a change to them without
his approval. This script makes sure a story cannot change state without the
file that carries the matching decision:

    to Planned    needs the work order            docs/work/<id>/work-order.md
    to Verified   needs an accepted work order, evidence and a safety review
    to Signed     needs the sign-off record       docs/signoff/<id>.md
    scope change  needs a change request          docs/changes/<name>.md

Usage:
    ledger_guard.py --base <commit> --head <commit>   a pull request
    ledger_guard.py --check <commit>                  one commit, consistency only

Standard library only. Reads everything through git, never the working folder.
"""

import argparse
import json
import re
import subprocess
import sys

BACKLOG = "docs/product/backlog.json"
STATUSES = ["Not started", "Planned", "Built", "Verified", "Signed", "Blocked"]
OWNER_ROLES = {"Owner", "Product owner"}
# A change to any of these is a change of scope, not of progress.
SCOPE_FIELDS = [
    "title", "ac", "points", "sprint", "epic", "role",
    "requirements", "serves", "unattended", "ownerAtMac", "evidence",
]


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, text=True)


def show(commit, path):
    """The text of a file at a commit, or None if it is not there."""
    result = git("show", f"{commit}:{path}")
    return result.stdout if result.returncode == 0 else None


def load_backlog(commit, problems):
    text = show(commit, BACKLOG)
    if text is None:
        return None
    try:
        data = json.loads(text)
    except json.JSONDecodeError as err:
        problems.append(f"{BACKLOG} is not valid JSON at {commit[:7]}: {err}")
        return None
    stories = data.get("stories") if isinstance(data, dict) else None
    if not isinstance(stories, list):
        problems.append(f"{BACKLOG} has no list of stories at {commit[:7]}.")
        return None
    by_id = {}
    for story in stories:
        sid = story.get("id")
        if not sid:
            problems.append("A story has no id.")
            continue
        if sid in by_id:
            problems.append(f"{sid} appears twice in the backlog.")
        by_id[sid] = story
    return by_id


def work_order(sid):
    return f"docs/work/{sid}/work-order.md"


def evidence(sid):
    return f"docs/work/{sid}/evidence.md"


def review(sid):
    return f"docs/work/{sid}/review.md"


def signoff(sid):
    return f"docs/signoff/{sid}.md"


def first_line_with(text, word):
    for line in text.splitlines():
        if word.lower() in line.lower():
            return line.replace("*", "").replace("`", "").strip()
    return ""


def verdict_is_verified(text):
    line = first_line_with(text or "", "verdict").upper()
    return "VERIFIED" in line and "NOT VERIFIED" not in line


def verdict_is_pass(text):
    line = first_line_with(text or "", "verdict").upper()
    return re.search(r"\bPASS\b", line) is not None and "BLOCK" not in line


def last_decision(text):
    """The last 'Decision:' line of a sign-off record: Approved, Interim, Rejected or ''."""
    found = ""
    for line in (text or "").splitlines():
        clean = line.replace("*", "").strip()
        match = re.match(r"^Decision:\s*(Approved|Interim|Rejected)\b", clean, re.I)
        if match:
            found = match.group(1).capitalize()
    return found


def check_consistency(commit, stories, problems):
    """Rules that must hold for any commit, whatever came before it."""
    for sid, story in stories.items():
        status = story.get("status")
        if status not in STATUSES:
            problems.append(
                f"{sid} has the status '{status}'. Allowed: {', '.join(STATUSES)}."
            )
        if status == "Signed":
            record = show(commit, signoff(sid))
            if record is None:
                problems.append(
                    f"{sid} is Signed but there is no sign-off record at {signoff(sid)}."
                )
            elif last_decision(record) not in ("Approved", "Interim"):
                problems.append(
                    f"{sid} is Signed but the last decision in {signoff(sid)} "
                    "is not Approved or Interim."
                )


def check_pull_request(base, head, problems, notes):
    merge_base = git("merge-base", base, head)
    before_commit = merge_base.stdout.strip() if merge_base.returncode == 0 else base
    diff = git("diff", "--name-only", before_commit, head)
    changed = set(diff.stdout.split("\n")) - {""}

    after = load_backlog(head, problems)
    if after is None:
        if not problems:
            problems.append(f"{BACKLOG} is missing from this pull request's branch.")
        return
    check_consistency(head, after, problems)

    before = load_backlog(before_commit, [])
    if before is None:
        notes.append("No backlog before this change, so only consistency was checked.")
        return

    change_requests = sorted(
        p for p in changed if p.startswith("docs/changes/") and p.endswith(".md")
    )
    scope_changes = []
    moved = 0

    for sid in sorted(set(before) | set(after)):
        old, new = before.get(sid), after.get(sid)
        if old is None:
            scope_changes.append(f"{sid} was added")
            continue
        if new is None:
            scope_changes.append(f"{sid} was removed")
            continue
        fields = [f for f in SCOPE_FIELDS if old.get(f) != new.get(f)]
        if fields:
            scope_changes.append(f"{sid} changed its {', '.join(fields)}")
        was, now = old.get("status"), new.get("status")
        if was == now:
            continue
        moved += 1
        check_move(sid, was, now, new, before_commit, head, changed, problems)

    # A sign-off record and the status must agree.
    for path in sorted(changed):
        match = re.match(r"^docs/signoff/(S[0-9]+-[0-9]+)\.md$", path)
        if not match:
            continue
        sid = match.group(1)
        record = show(head, path)
        if record is None:
            problems.append(
                f"The sign-off record {path} was deleted. Records are kept, never removed."
            )
            continue
        decision = last_decision(record)
        status = after.get(sid, {}).get("status")
        if not decision:
            problems.append(f"{path} has no line 'Decision: Approved', 'Interim' or 'Rejected'.")
        elif decision == "Rejected" and status == "Signed":
            problems.append(f"{path} says Rejected but {sid} is Signed.")
        elif decision in ("Approved", "Interim") and status != "Signed":
            problems.append(f"{path} says {decision} but {sid} is '{status}', not Signed.")

    if scope_changes and not change_requests:
        shown = "; ".join(scope_changes[:6])
        more = f"; and {len(scope_changes) - 6} more" if len(scope_changes) > 6 else ""
        problems.append(
            "The scope of the backlog changed (" + shown + more + ") without a change "
            "request. Add one under docs/changes/, which the owner must approve."
        )

    notes.append(f"{len(after)} stories, {moved} moved, {len(scope_changes)} scope changes.")


def check_move(sid, was, now, story, before_commit, head, changed, problems):
    owner_story = story.get("role") in OWNER_ROLES
    move = f"{sid}: {was} to {now}"

    if now not in STATUSES:
        return  # already reported by the consistency check

    if was == "Signed":
        if signoff(sid) not in changed:
            problems.append(
                f"{move}. A signed story is reopened only with a new entry in "
                f"{signoff(sid)}, which the owner must approve."
            )
        return

    if now == "Not started":
        problems.append(f"{move}. A story does not go back to Not started.")

    elif now == "Planned":
        if work_order(sid) not in changed and signoff(sid) not in changed:
            problems.append(
                f"{move}. A story becomes Planned only in a pull request that adds or "
                f"changes its work order ({work_order(sid)}), or that records a "
                f"rejection in {signoff(sid)}. The owner must approve either one."
            )

    elif now == "Built":
        problems.append(
            f"{move}. Built is not merged. A build pull request is merged only when "
            "the story is Verified."
        )

    elif now == "Verified":
        if was not in ("Planned", "Built"):
            problems.append(f"{move}. Only a Planned story can become Verified.")
        if show(before_commit, work_order(sid)) is None:
            problems.append(
                f"{move}. Its work order was not accepted first: {work_order(sid)} is "
                "not on the branch this pull request goes into."
            )
        if not verdict_is_verified(show(head, evidence(sid))):
            problems.append(
                f"{move}. {evidence(sid)} is missing or its verdict is not VERIFIED."
            )
        if not verdict_is_pass(show(head, review(sid))):
            problems.append(
                f"{move}. {review(sid)} is missing or its verdict is not PASS."
            )

    elif now == "Signed":
        if was != "Verified" and not owner_story:
            problems.append(f"{move}. Only a Verified story can be Signed.")
        if signoff(sid) not in changed:
            problems.append(
                f"{move}. Signing needs the sign-off record {signoff(sid)} added or "
                "changed in the same pull request, which the owner must approve."
            )

    # now == "Blocked" is always allowed.


def main():
    parser = argparse.ArgumentParser(description="Ledger rules for KUE")
    parser.add_argument("--base")
    parser.add_argument("--head")
    parser.add_argument("--check")
    args = parser.parse_args()

    problems, notes = [], []
    if args.check:
        stories = load_backlog(args.check, problems)
        if stories is None:
            if not problems:
                notes.append(f"No {BACKLOG} at this commit. Nothing to check.")
        else:
            check_consistency(args.check, stories, problems)
            notes.append(f"{len(stories)} stories, consistency only.")
    elif args.base and args.head:
        check_pull_request(args.base, args.head, problems, notes)
    else:
        parser.error("give --base and --head, or --check")

    for note in notes:
        print(note)
    if problems:
        print(f"\nLedger rules: FAILED, {len(problems)} problem(s).\n")
        for number, problem in enumerate(problems, 1):
            print(f"{number}. {problem}")
        return 1
    print("Ledger rules: passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
