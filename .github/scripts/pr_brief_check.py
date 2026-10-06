#!/usr/bin/env python3
"""Owner brief check for KUE.

Every pull request must start with a brief for the owner: what it is, what
changes if he approves, what does not change, why, how it was checked, what was
not checked, what happens after, how to undo it, and what he must do.

This check reads the pull request's description and refuses it when a part is
missing, empty, or still holds a placeholder. It cannot judge whether the brief
is true or clear. That is the owner's to judge when he reads it.

Usage:
    pr_brief_check.py                 reads the pull request from $GITHUB_EVENT_PATH
    pr_brief_check.py --file <path>   reads a description from a file (for testing)

Standard library only.
"""

import json
import os
import re
import sys

PARTS = [
    "What this is",
    "What changes if you approve",
    "What does not change",
    "Why",
    "How it was checked",
    "Not checked",
    "After you approve",
    "To undo",
    "Your action",
]
ACTIONS = ("approve", "test first", "none")
HEADING = "## For the owner"
MAX_WORDS = 350


def read_body():
    if len(sys.argv) == 3 and sys.argv[1] == "--file":
        with open(sys.argv[2], encoding="utf-8") as handle:
            return handle.read()
    path = os.environ.get("GITHUB_EVENT_PATH")
    if not path:
        print("No pull request to read. Give --file <path> or run inside GitHub Actions.")
        sys.exit(2)
    with open(path, encoding="utf-8") as handle:
        event = json.load(handle)
    return (event.get("pull_request") or {}).get("body") or ""


def strip_comments(text):
    return re.sub(r"<!--.*?-->", "", text, flags=re.S)


def brief_section(body):
    """The text from '## For the owner' to the next '## ' heading."""
    lines = body.splitlines()
    start = next((i for i, line in enumerate(lines) if line.strip().lower() == HEADING.lower()), None)
    if start is None:
        return None
    end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith("## ")), len(lines))
    return "\n".join(lines[start + 1:end])


def split_parts(section):
    """Map each '**Label:**' to the text that follows it, up to the next label."""
    found = {}
    current = None
    for line in section.splitlines():
        match = re.match(r"^\s*\*\*(.+?):\*\*\s*(.*)$", line)
        if match and match.group(1).strip() in PARTS:
            current = match.group(1).strip()
            found[current] = match.group(2).strip()
        elif current is not None:
            found[current] = (found[current] + "\n" + line).strip()
    return found


def is_placeholder(text):
    """True when the text is empty or is only a template placeholder such as <one sentence>."""
    cleaned = re.sub(r"^[-*]\s*", "", text.strip(), flags=re.M).strip()
    if not cleaned:
        return True
    return re.search(r"<[^<>\n]{3,}>", cleaned) is not None


def main():
    body = strip_comments(read_body())
    problems = []

    section = brief_section(body)
    if section is None:
        print("Owner brief: FAILED.\n")
        print(f"1. The description has no section headed '{HEADING}'. "
              "Every pull request starts with the brief in .github/pull_request_template.md.")
        return 1

    parts = split_parts(section)
    for label in PARTS:
        if label not in parts:
            problems.append(f"The part '{label}:' is missing.")
        elif is_placeholder(parts[label]):
            problems.append(f"The part '{label}:' is empty or still holds a placeholder.")

    action = parts.get("Your action", "").strip().lower()
    if action and not is_placeholder(parts.get("Your action", "")) and not action.startswith(ACTIONS):
        problems.append("'Your action:' must start with Approve, Test first, or None.")

    words = len(re.findall(r"\S+", section))
    if words > MAX_WORDS:
        problems.append(
            f"The brief is {words} words. Keep it to one screen, at most {MAX_WORDS}; "
            "put the rest under '## Details'."
        )

    if problems:
        print(f"Owner brief: FAILED, {len(problems)} problem(s).\n")
        for number, problem in enumerate(problems, 1):
            print(f"{number}. {problem}")
        return 1
    print(f"Owner brief: passed. {len(PARTS)} parts, {words} words.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
