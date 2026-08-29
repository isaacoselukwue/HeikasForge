# Agent security

## Capability model

An agent never receives ambient authority. Each invocation carries an explicit tool policy: whether it may read, search, inspect Git, write, delete or patch, which named command identifiers it may run, the path policy, and a maximum tool call count.

Planning and review are read-only. Implementation and repair may edit only the assigned candidate worktree.

## No general shell

There is no shell, exec or system tool. The only way to run anything is `run_named_command` with a command identifier that appears in the invocation's allowlist and in the effective configuration. The command is executed as an executable plus an argument vector. Task text is never concatenated into a command line.

## Path confinement

Every path an agent supplies is parsed into a relative workspace path first. A path is rejected if it is empty, absolute, contains a parent component, carries a drive prefix or contains a control character. It is then evaluated against sensitive globs, which deny even reading, and protected globs, which deny writing and deleting unless the approved plan named the path.

The surviving path is joined to the canonicalised worktree root, the nearest existing ancestor is canonicalised so symbolic links are resolved, and the result must still be inside the root. A symbolic link pointing out of the worktree is therefore refused.

## Evidence over claims

The result of an implementation or repair node comes from the Git diff and the process evidence, not from the agent's summary. A read-only node that changed files is a policy violation and fails the node.

## Prompts

Prompts are rendered from versioned templates. Each carries the role, the approved plan hash, the allowed and forbidden actions, bounded evidence from the previous node and the exact completion schema. The template hash is recorded in the attempt evidence. Secrets are never placed in a prompt.

## External adapters

An external coding CLI adapter detects its executable, maps the tool policy onto the strongest restrictions that the CLI genuinely supports, and reports its actual isolation strength. If it cannot honour a required boundary, such as the read-only planning role, the invocation is refused rather than downgraded. The product never claims sandboxing it does not have.

## Demonstration driver

The deterministic driver refuses to operate on any worktree that does not carry a `.heikas-fixture` marker, and the interface shows a persistent demonstration badge whenever it is active. It cannot be selected for a real run unless demonstration mode is explicitly requested.
