---
name: kue-sign
description: Record the owner's signature on a verified KUE story. Only the owner runs this, by typing it himself.
argument-hint: "<story id> [interim | reject <reason>]"
disable-model-invocation: true
---

# The owner signs a story

Arguments given: `$ARGUMENTS`

This command records the owner's decision. Run it only because the owner typed it in this session. Never run it in an unattended run, and never on your own initiative.

1. Find the story in `docs/product/backlog.json`. Its status must be `Verified`. If it is not, say what its status is and stop.
2. Show the owner, in plain words, from `docs/work/<id>/evidence.md` and `docs/work/<id>/review.md`:
   - what was proved
   - what was not checked
   - the live checks that were waiting for him, and ask whether he did them and what he saw
3. Ask him to confirm: Approved, Interim, or Rejected. Use the argument if he gave one, and still show step 2 first.
4. Record it on `kue/release-1`:
   - **Approved:** status `Signed`, with `signedAt` and `warrant: "Approved"`.
   - **Interim:** status `Signed`, with `signedAt`, `warrant: "Interim"` and his condition in `note`.
   - **Rejected:** status `Planned`, with his reason in `note`. Add the reason to the top of `docs/work/HANDOVER.md` so the next run starts from it.
5. Commit: `sign(<id>): owner <Approved | Interim | Rejected>`.
6. Tell him the new percent complete and what `/kue-next` would take now.
