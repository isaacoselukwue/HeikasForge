# Graph and reducer

## Nodes

Fourteen nodes are registered. Each declares its scope, class, whether it is read-only and the exact set of successors it may route to. `NodeResult::validate` rejects any `next` value outside that set, so an agent or a bug cannot invent a transition.

Run-scoped nodes are `prepare`, `plan`, `approval`, `fan_out`, `join`, `integrate_winner`, `final_test`, `final_review`, `commit_approval` and `commit`. Candidate-scoped nodes are `implement_candidate`, `test_candidate`, `review_candidate` and `repair_candidate`.

`integrate_winner` may route to itself. That edge is candidate promotion after a patch fails to apply.

Two tests keep the declared graph and the node contracts in agreement: every declared edge must be an allowed successor, and every allowed successor must be a declared edge.

## Scheduler

The next step is derived from the projection rather than stored, so a restart cannot disagree with the durable history. The order is: cancellation, prepare, the plan gate, fan out, candidate subgraphs, join, integration, the final gates, then the commit policy.

The plan gate is a small state machine over the plan history. No plan version means write one. A revision request against the current version means write another. An approval whose hash equals the current version's hash means proceed. Anything else means pause for the operator.

Each candidate is driven by its own task under a semaphore sized from the configured parallelism and the available processors. A candidate's next node is the `next` value recorded on its most recent closed attempt, so a candidate that crashed mid-flight resumes at the right place.

## Reducer

`RunProjection::apply` verifies the event against the expected sequence and previous hash before reducing it, so an out-of-order or corrupt event can never mutate state. The reducer is exhaustive over the event payload and rejects illegal run and candidate transitions, duplicate node attempts, closing an attempt that never started, and events for another run.

Failure classification decides routing. Only a transient infrastructure failure retries the same node, bounded by the retry policy with exponential backoff and full jitter. A task failure routes to the repair node while budget remains and otherwise fails the candidate. A policy violation, a permanent configuration failure or an internal invariant never retries.

A repair budget is exhausted either by attempt count or early, after two consecutive repairs that produced an unchanged failure fingerprint. That stops a candidate looping on the same failure.
