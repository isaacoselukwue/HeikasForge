# Architecture records

These documents record the durable reasoning behind the implementation. They describe contracts and invariants rather than restating source code.

| Record | Subject |
| --- | --- |
| [event-protocol.md](event-protocol.md) | The durable event log, hash chain and atomic persistence protocol |
| [graph-reducer.md](graph-reducer.md) | The node contracts, scheduler and state reducer |
| [recovery-playbook.md](recovery-playbook.md) | What recovery does at every persistence boundary |
| [agent-security.md](agent-security.md) | The agent capability model and path confinement |
| [worktree-lifecycle.md](worktree-lifecycle.md) | Git isolation, integration and commit rules |
| [quality-providers.md](quality-providers.md) | The stable review report and provider contract |
| [candidate-selection.md](candidate-selection.md) | Eligibility and the deterministic score tuple |
| [local-api.md](local-api.md) | Loopback delivery, session security and event streaming |
| [media-capture.md](media-capture.md) | How the documentation media is produced |
| [repository-policy.md](repository-policy.md) | The conformance checks and their documented exemptions |
