# Recovery playbook

## When recovery runs

Recovery runs at the start of every dispatch, before any node executes. It is ordinary behaviour, not an exceptional path.

## Steps

1. Acquire the operating system advisory lock on `locks/dispatcher.lock`. Only one dispatcher may mutate a run.
2. Verify the event chain from sequence one. Quarantine and truncate a partial final record if one exists.
3. Load `state.json`. If it is missing, start from a genesis projection. If it claims a sequence beyond the durable log, stop and report a corrupt log rather than guessing.
4. Replay every event newer than the projection's sequence.
5. Find every attempt with a `NodeStarted` and no terminal event. For each, append `NodeInterrupted`. Move any candidate that was mid-flight to `interrupted`.
6. Append `RecoveryStarted` and `RecoveryCompleted` around that work so the timeline shows what happened.
7. Store the repaired projection and metrics.

## Guarantees

- A completed node is never repeated because the dispatcher restarted. The scheduler derives the next step from recorded successful attempts.
- A crash after a terminal event but before the projection was replaced is repaired by replay alone. No node reruns.
- A crash after `NodeStarted` marks that attempt interrupted and retries the node with the next attempt number. The interrupted attempt stays in the history.
- Evidence is never removed automatically. Worktrees are retained after failure until the operator exports or cleans them.
- A corrupt chain moves the run to `recovery_required` and presents an explicit export path rather than continuing silently.

## Tested boundaries

The suite covers an attempt interrupted after `NodeStarted`, a projection deliberately rewound behind the durable log, a dispatcher restart between candidate steps, a paused run resumed from files alone by a fresh process, and a forced `SIGKILL` of the orchestrator followed by `heikas resume`.
