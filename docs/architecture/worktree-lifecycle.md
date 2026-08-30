# Worktree lifecycle

## Deterministic checkout

Every Git invocation the service makes carries per-invocation overrides that turn off line ending translation. A candidate worktree therefore holds exactly the bytes of the baseline commit on every platform, so diffs, changed line counts and content hashes mean the same thing on Windows as they do elsewhere. Nothing in the user's own Git configuration is modified: the settings are supplied on the command line for the duration of each call.

## Baseline

Every candidate starts from one immutable baseline commit. When the operator opts into including uncommitted work, a binary patch of tracked changes and a compressed archive of untracked files are captured once and stored as run artefacts, then applied identically to every candidate worktree. The user's branch is never committed to in order to take that snapshot.

## Location

Candidate and integration worktrees live under the application data root, never inside one another and never inside the source repository. Branch names are private to the run.

## Diffing

A candidate diff is produced by staging everything in the candidate worktree and taking a binary-safe diff of the index against the baseline. Staging inside a private worktree is a Git service operation; agents never stage or commit. Ignored files stay ignored, so build output and reports do not pollute a candidate diff.

## Integration

The winning patch is applied to an integration worktree that has been reset hard to the baseline and cleaned. Every required test and review runs there again. Candidate gate results are never reused as final evidence.

If the patch does not apply, no agent is invoked. The candidate is recorded as non-promotable with an integration failure reason, and the next ranked candidate is promoted. If none remains, the run ends as exhausted.

## Commit

The commit node creates or resets `heikas/run-<short-run-id>`, stages only the paths present in the integration diff, checks the protected path policy again, and commits with author and committer name `Isaac Oselukwue` and the email already configured in the repository. No email is ever invented; a repository without one pauses for user action instead.

The repository's signing configuration is preserved. If signing cannot run without interaction, the run surfaces a user action state rather than disabling signing silently.

Nothing is pushed, and the default branch is never modified.

## Cleanup

Worktrees survive failure so evidence can be inspected. Removal is explicit through `heikas cleanup`, requires the run to be terminal, and preserves the run evidence directory.
