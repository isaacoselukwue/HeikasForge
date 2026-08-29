# Candidate selection

## Eligibility

A candidate is eligible only when all of the following hold: every required test command passed, every required review provider passed, no blocker policy finding exists, a non-empty valid diff exists unless the task correctly requires no source change, the diff applies cleanly to the baseline, every required report is present and valid, and the candidate stayed inside its repair, time and resource budgets.

Every failed condition produces a typed exclusion reason with a plain-language summary. The reasons are persisted on the candidate and shown in the interface. Nothing is excluded without a recorded cause.

## Ordering

Eligible candidates are sorted lexicographically, lowest first, by this tuple:

1. blocker issue count
2. critical issue count
3. high issue count
4. medium issue count
5. new security issue weighted score
6. new reliability issue weighted score
7. new maintainability issue weighted score
8. coverage rank
9. changed test integrity penalty
10. total changed lines
11. repair attempt count
12. total gate duration
13. candidate identifier

Severity weights rise geometrically so that one higher-severity issue always outranks any number of lower-severity ones within a component.

## Coverage rank

Coverage is encoded so that ordinary integer comparison gives the documented behaviour. Measured coverage becomes the negated scaled percentage, so higher coverage sorts first. Missing coverage is a distinct variant that sorts after every measured value, making it worse than any measured passing coverage. Coverage measured below a required threshold never reaches ranking at all because it makes the candidate ineligible.

## Determinism

The candidate identifier is the final component, so the order is total and repeated evaluation of the same evidence produces the same winner. The complete ranking, every score tuple and every exclusion reason are persisted to `integration/ranking.json`.

The rationale shown to the operator is derived from those facts. It names the first tuple component where the winner and the runner-up differ, and lists the winning tuple. It invents no qualitative claim.

## Promotion

If the winner fails to apply, or fails a final gate in the integration worktree, it is marked non-promotable and the next ranked promotable candidate is tried. Each promotion resets the final gate outcomes so a later candidate is judged on its own evidence. When no candidate remains, the run ends as exhausted rather than committing anything.
