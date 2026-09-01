# Contributing to Heikas Forge

Thank you for considering a contribution. This document describes how to get a working
development environment, what the project expects of a change, and how a change is
reviewed.

Everyone taking part is expected to follow the [code of conduct](CODE_OF_CONDUCT.md).

## Getting set up

Requirements: Rust 1.98, Node.js 22, pnpm, Git and Python 3 for the bundled fixture.
`ffmpeg` is needed only if you regenerate the documentation video.

```bash
git clone https://github.com/isaacoselukwue/HeikasForge.git
cd HeikasForge
pnpm install
pnpm --dir apps/web build
cargo build --release -p heikas-cli
```

Run the deterministic demonstration to confirm the whole pipeline works on your machine.
It needs no model, no account and no network.

```bash
cargo run -p xtask -- demo
```

## Before you open a pull request

Run the complete verification suite. It is the same set of checks that continuous
integration runs, so a green run locally means a green run on the pull request.

```bash
cargo run -p xtask -- verify
```

It covers formatting, lint with warnings denied, the Rust test suite, the interface lint,
type check, format check and tests, the integration tests, the repository policy checks,
the interface bundle build, schema and wire type drift, dependency advisories and
licences, the browser suite, the public media validation, the authorship check and a
release build.

If you changed a serialisable application type or a JSON schema, regenerate the published
schemas and the generated wire types, and commit them:

```bash
cargo run -p xtask -- schemas
```

## What the project expects of a change

These rules are enforced by the repository policy crate, so a change that breaks one will
fail `cargo run -p xtask -- verify` before it reaches review.

- **British English** in documentation, interface copy, command line output, errors and
  commit messages. Prefer `optimise`, `authorise`, `colour`, `centre`, `licence` for the
  noun, `cancelled` and `initialise`.
- **No em dash** in any tracked text file. Use a full stop, colon, comma or ordinary
  hyphen.
- **No comments in first-party source.** That includes line comments, block comments,
  documentation comments, commented out code and task markers such as `TODO`, `FIXME` or
  `HACK`. Use names, types and function boundaries that make the code readable, and put
  durable reasoning in the architecture records under `docs/architecture/`.
- **No linter suppression comments.** Refactor the code, or configure the tool centrally
  with a documented reason.
- **No unchecked `unwrap`, `expect` or panic driven control flow** in production Rust.
  Test code may use concise assertion helpers where failure is the intended behaviour.
- **No vague module names** such as `utils`, `helpers`, `manager`, `misc` or `common`.
- **Do not weaken an existing test.** Deleting a test, adding a skip marker, lowering a
  coverage threshold or disabling a rule is a blocking finding. If a test is genuinely
  wrong, correct it and say why in the commit message.

## Architecture boundaries

The workspace is layered, and the layering is not advisory.

| Crate | Owns |
| --- | --- |
| `domain` | State machines, value objects, events, score policy and invariants |
| `application` | Use cases and ports |
| `infrastructure` | Files, Git, processes, agents, reports and operating system integration |
| `api` | Loopback delivery, session security and event streaming |
| `cli` | Terminal delivery and exit codes |
| `policy` | Repository wide conformance checks |
| `apps/web` | The graphical interface |

The domain layer must not import HTTP, filesystem, Git, process, logging implementation
or frontend concerns. The command line and the API must call the same application
services, and neither may embed domain behaviour of its own.

Introduce an interface, trait or generic only for a real variability point, policy
boundary or external dependency.

## Security sensitive areas

Some parts of the system carry rules that are easy to break without noticing. Read
[docs/architecture/agent-security.md](docs/architecture/agent-security.md) before touching
any of them.

- A repository's own `.heikas/forge.toml` is untrusted input. Repository content may never
  decide which program runs, with which arguments, in which directory, outside an explicit
  trust decision.
- Never execute task text through a shell. A command is an executable plus an argument
  vector.
- A project detector may propose only programs from a fixed compile time set of bare
  executable names, and only arguments that are compile time constants.
- A test gate that executed no tests is not evidence and must not pass.
- Redact before anything is written to a log, an event, an artefact or an export.

## Tests

A change is expected to arrive with the tests that prove it. The suite is organised by
what it proves rather than by the file it covers, so prefer adding to an existing file
whose subject matches.

Name a test as a sentence describing the behaviour it protects, in the style of the
existing suite, for example `a_suite_that_runs_no_tests_is_not_recorded_as_passing`.

Where a change concerns an external tool's output, capture that output from a real run and
assert against the captured text rather than against an assumed format.

## Commit messages

Write in British English. Explain what changed and why, in prose, wrapped at a sensible
width. Do not add co-author trailers, generated by trailers, or any assistant, model or
tool attribution.

## Reporting a security issue

Do not open a public issue for a security problem. Follow [SECURITY.md](SECURITY.md).

## Licence

By contributing you agree that your contribution is licensed under the MIT licence that
covers this repository.
