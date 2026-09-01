# Security policy

## Supported versions

Heikas Forge is developed on the `main` branch, and security fixes are applied there.
Tagged releases are supported until the next tagged release supersedes them.

| Version | Supported |
| --- | --- |
| `main` | Yes |
| Latest tag | Yes |
| Earlier tags | No |

## Reporting a vulnerability

Please do not open a public issue, a pull request or a discussion for a security problem.

Report privately through GitHub, using **Security** then **Report a vulnerability** on
this repository. That opens a private advisory visible only to you and the maintainers.

Direct link:
<https://github.com/isaacoselukwue/HeikasForge/security/advisories/new>

Please include:

- what an attacker can achieve, stated as an outcome rather than as a code observation;
- the exact steps or the repository shape that triggers it, ideally a minimal example;
- which version, commit or tag you tested;
- your operating system and toolchain versions;
- whether the problem is reachable on the documented free local path, or only with an
  optional adapter configured.

You will get an acknowledgement within seven days. If a report is confirmed, you will be
told the intended fix and the expected timing, and you will be credited in the advisory
unless you ask not to be.

## What is in scope

Heikas Forge runs on your own machine, orchestrates coding agents, and executes commands
from configuration. The boundaries below are the ones the design commits to, and a way
past any of them is a vulnerability.

- **Repository configuration is untrusted.** A repository's own `.heikas/forge.toml` may
  not decide which program runs, with which arguments, or in which directory, without an
  explicit per digest trust decision made with `heikas trust`. It may not redirect the
  model endpoint, the credential variable, the Sonar host or the commit authorship at any
  trust level, and it may only tighten a safety setting, never relax one.
- **Agents receive no general shell.** Only named commands from the effective
  configuration can run, as an executable plus an argument vector. Task text is never
  interpolated into a shell.
- **Agent paths are confined.** Every path an agent supplies is confined to its assigned
  worktree and checked against protected and sensitive glob rules. A symbolic link that
  leaves the worktree is refused, as is a patch that renames, copies or changes the mode of
  a protected path, and one that creates a symbolic link.
- **Git writes belong to the infrastructure Git service.** An agent may not commit, push,
  fetch, merge, rebase, alter remotes, change Git configuration or update submodules. The
  default branch is never modified.
- **The interface is loopback only.** The local API binds to `127.0.0.1`, checks the host
  and the origin, requires a session cookie and a cross site token for a mutation, and
  rate limits both session establishment and mutations.
- **Secrets are redacted before durability.** Nothing unredacted should reach the event
  log, a projection, attempt evidence, the live event stream or an export.
- **A test gate that executed nothing is not evidence** and must not be recorded as
  passing.

A way to defeat any of the above, to read a file outside a worktree, to execute a program
that no one configured or trusted, or to have a secret written to disk unredacted, is in
scope.

## What is not in scope

- **Running a project's own build tool executes that project's code.** Compiling and
  running a repository's tests is the entire purpose of the product. A repository can
  therefore choose what its test process prints, including its reported count of executed
  tests, through a build tool configuration file, a custom test harness or a fixture that
  runs at collection time. The executed count is a guard against a repository that has no
  tests, not a guard against one that lies. This is documented in
  [docs/architecture/quality-providers.md](docs/architecture/quality-providers.md).
- **An operator's own user configuration is trusted.** Settings you place in your own
  configuration are yours, including a model endpoint, a credential variable name and any
  command you declare.
- **A command you declare for a single run with `--command`** is trusted for that run,
  because it comes from your own shell. This is deliberately available only in the
  terminal and is refused by the interface.
- **Optional adapters that require an account**, and the services behind them, are outside
  this project's control. A weakness in an external coding command line interface or in a
  hosted model provider should be reported to that project.
- Denial of service through resource exhaustion caused by a repository you deliberately
  pointed the tool at, where no confinement boundary is crossed.

## Verifying a release

Every commit introduced to this repository carries a single author and committer. The
authorship check runs in continuous integration and can be run locally:

```bash
cargo run -p xtask -- authorship
```

The repository conformance checks, which include rules against committed host paths and
committed secret material, run with:

```bash
cargo run -p heikas-cli -- policy .
```
