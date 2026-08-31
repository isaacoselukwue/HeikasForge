# Quality providers

## What a test gate proves, and what it does not

A gate that exits zero having executed nothing is not evidence. Every test command the
detector proposes therefore declares a report format that yields a count of executed
tests, read from the command's own standard output, and a required test command that
executed nothing is recorded as missing evidence rather than as a pass. Skipped and
ignored tests do not count as executed, so a suite in which every test is skipped fails
the same way.

Where no count can be obtained the detector declines rather than proposing a gate it
cannot check. That is why a Node project is declined even when it declares a test script:
the script is an opaque command whose executed count is not reported, so a stub that exits
without running anything could not be told apart from a passing suite. The operator can
still declare such a command deliberately, which is a decision they make with full sight
of it.

The count defends against absence, not against a hostile repository. Running a project's
own build tool executes code from that repository by construction: compiling and running
its test binaries is the entire point. A repository can therefore choose what its test
process prints, and the count is only as trustworthy as the repository it came from. It
is a guard against a repository that has no tests, not a guard against one that lies.
Nothing downstream should be read as a claim that a count is tamper resistant.

## The stable report comes first

Every provider normalises into one `ReviewReport` shape before any provider-specific logic exists. A report carries its schema version, the provider name, whether it is required or advisory, whether it passed, the quality gate outcome, its issues, its metrics, its artefact references and its start and finish times.

An issue carries the provider, a stable fingerprint, a rule identifier, a category, a severity, an optional file, line and column, a message, an optional help reference and whether it is new relative to the baseline.

A report is validated on arrival. A report that claims to have passed while carrying a failed quality gate is rejected as an invariant violation.

## The local provider is mandatory

The local provider has no paid dependency and is always available. It runs the configured review-phase commands, which cover format checking, linting, dependency audit, secret scanning, static analysis producing SARIF and repository policy. A required command that fails, times out or does not produce its declared report becomes a blocker issue and fails the gate. A missing required report is a failure, never a pass.

The strict profile requires a command for each of format, lint, audit, secret scan, static analysis and policy. If one is absent, preparation refuses to start the run rather than quietly running fewer gates.

## Report parsing

JUnit XML, LCOV, SARIF and the Cargo test JSON stream are parsed into the shared model. Coverage is read from LCOV line counts. SARIF results become issues with their rule identifiers, levels, locations and help references preserved.

## Test integrity

The local provider compares every changed test file and every changed quality configuration file against its baseline content. It reports a blocker when a test file is deleted, when the number of declared tests falls, when skip markers are added, or when a coverage threshold is lowered. A large fall in assertion count is reported as high severity.

If the approved plan explicitly named the path, the finding is downgraded to advisory and the message says so. That is the only way an existing test may legitimately change.

## Optional adapters

The SonarQube scanner adapter runs a self-managed scanner with quality-gate waiting and normalises the outcome. The SonarQube MCP adapter must record actual tool calls for the quality gate, issues and security hotspots; missing tool evidence is a provider failure, not a pass. Neither is required, and their absence does not reduce the local path.

## Advisory review

An advisory review provider may add maintainability and design findings. Its findings are capped at medium severity unless a configured deterministic rule promotes a specific rule identifier to a blocker. It can never override a deterministic test or scanner result, and it is never the only required provider.
