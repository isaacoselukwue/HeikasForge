# Agent security

## Capability model

An agent never receives ambient authority. Each invocation carries an explicit tool policy: whether it may read, search, inspect Git, write, delete or patch, which named command identifiers it may run, the path policy, and a maximum tool call count.

Planning and review are read-only. Implementation and repair may edit only the assigned candidate worktree.

## No general shell

There is no shell, exec or system tool. The only way to run anything is `run_named_command` with a command identifier that appears in the invocation's allowlist and in the effective configuration. The command is executed as an executable plus an argument vector. Task text is never concatenated into a command line.

## Repository configuration is untrusted input

`.heikas/forge.toml` lives inside the target repository, so it is treated as hostile input rather than as a trusted layer. Configuration is resolved from two layers with different authority.

Your own user configuration has full authority. Repository configuration is constrained in three ways.

Settings that name an executable or its arguments are honoured only after an explicit, per repository trust decision. That covers `[[commands]]`, `agent.driver`, `agent.fixture_script` and the scanner programs and arguments. Trust is recorded against the exact digest of the configuration file, so editing the file withdraws the decision until it is granted again. `heikas init` grants trust for the file it writes, because writing it is itself a deliberate act by the owner.

Settings that would redirect your credentials, your network traffic or your authorship are never honoured from a repository, at any trust level. That covers `agent.endpoint`, `agent.api_key_environment_variable`, `agent.executable`, `agent.extra_arguments`, the Sonar host and token variables, `git.author_name`, `git.include_dirty` and `run.commit_policy`.

Settings that carry a safety meaning may only be tightened. Protected and sensitive path lists are unioned rather than replaced, read and write limits may only fall, the environment allowlist may only narrow, and a repository can never turn off redaction, test protection, the clean working tree requirement or a quality profile.

Whatever is withheld is recorded on the effective configuration and reported by `heikas doctor` with the reason, so a repository can never quietly lose a setting. Trust is granted from the terminal with `heikas trust <repository>` and never from the browser.

## Named commands and diagnostics

`heikas doctor` probes the executable behind every configured command. Because an untrusted repository cannot contribute a command program, that probe can only ever run something you configured or something the project detector proposed.

## Path confinement

Every path an agent supplies is parsed into a relative workspace path first. A path is rejected if it is empty, absolute, contains a parent component, carries a drive prefix or contains a control character. It is then evaluated against sensitive globs, which deny even reading, and protected globs, which deny writing and deleting unless the approved plan named the path.

The surviving path is joined to the canonicalised worktree root, the nearest existing ancestor is canonicalised so symbolic links are resolved, and the result must still be inside the root. A symbolic link pointing out of the worktree is therefore refused.

## Patch confinement

A patch is never inspected by scanning its text for header lines. The tool writes the patch to a temporary file and asks Git itself to enumerate the paths it would touch, forwards and in reverse, so renames, copies, mode changes, creations and deletions are all reported. Every path Git names is then evaluated against the same path policy as a direct write. A patch that would create or alter a symbolic link is refused outright.

## Evidence over claims

The result of an implementation or repair node comes from the Git diff and the process evidence, not from the agent's summary. A read-only node that changed files is a policy violation and fails the node.

Change detection hashes file content. It does not use size and modification time, because a process can rewrite a file at equal length and restore its timestamp.

## Prompts

Prompts are rendered from versioned templates. Each carries the role, the approved plan hash, the allowed and forbidden actions, bounded evidence from the previous node and the exact completion schema. The template hash is recorded in the attempt evidence. Secrets are never placed in a prompt.

## External adapters

An external coding CLI adapter detects its executable, then reads the interface's own option listing to confirm that the restriction flags it depends on are actually accepted by the installed version. Isolation strength is derived from that detection, never asserted from the adapter kind. If the listing cannot be read, or a required option is missing, the adapter reports no isolation and refuses any read-only role rather than downgrading it.

Extra arguments supplied by the operator are validated before the adapter is constructed. An argument that names a known restriction bypass, or that collides with an option the adapter sets itself, is refused with a typed configuration error. Safety flags are emitted after the operator's arguments so that a later occurrence wins where the interface's parser resolves duplicates that way.

After a read-only invocation the adapter compares content hashes across the worktree. If anything changed, the invocation fails as a policy violation, because the restriction was not effective in practice whatever the interface advertised.

Where the interface supports it, the prompt is delivered on standard input rather than in the argument vector, so task text is not visible to other local processes through the process listing. An adapter that cannot do this says so in its diagnostics.

## Demonstration driver

The deterministic driver refuses to operate on any worktree that does not carry a `.heikas-fixture` marker, and the interface shows a persistent demonstration badge whenever it is active. It cannot be selected for a real run unless demonstration mode is explicitly requested.
