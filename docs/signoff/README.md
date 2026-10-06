# Sign-off records

One file per story: `docs/signoff/<story id>.md`. It is the record of what the owner tested himself and what he decided. Only the owner can approve a change in this folder, and his approval of the pull request that adds the record is his signature.

A record is never deleted or rewritten. If a story is tested again, a new section is added below the old one. The last `Decision:` line in the file is the one in force.

`/kue-sign <story id>` writes the record from what the owner reports. Nothing is filled in for him.

## The form

```
# Sign-off record: <story id> <story title>

## <date>, first test

| | |
| --- | --- |
| Build tested | commit <first 7 characters> on kue/release-1 |
| Work order accepted | pull request #<n> |
| Built and verified | pull request #<n>. Checks: Ledger rules, Core tests, Window tests |
| Tested by | the owner, at his Mac |

### What the owner tested

| # | What he did | What he should see | What he saw | Result |
| --- | --- | --- | --- | --- |
| 1 | | | | Passed / Failed / Not done |

### Not tested, and why

<Each acceptance check the owner did not test himself, and what covers it instead. Write "nothing" if nothing.>

### Decision

Decision: Approved | Interim | Rejected

Condition or reason: <for Interim, the condition and its date; for Rejected, the reason; for Approved, "none">

### Signature

The owner signs by approving the pull request that adds this section. GitHub keeps who approved and when.
```

## The three decisions

| Decision | Means | The story becomes |
| --- | --- | --- |
| Approved | Every check he tested passed | Signed |
| Interim | He accepts it with a named condition and a date | Signed, with the condition on the record |
| Rejected | It is not what he asked for, or a check failed | Planned again, with his reason |

Approved is not offered when a check he tested failed or was not done.
