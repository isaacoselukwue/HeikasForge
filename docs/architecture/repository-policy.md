# Repository policy

## Rules

The policy crate inspects the tracked files and the commit history and fails on:

- the Unicode em dash character in any tracked text file;
- a comment in first-party Rust, TypeScript, JavaScript, CSS or shell source;
- a task marker in first-party source;
- a vague container module name such as `utils`, `helpers`, `manager`, `misc` or `common`;
- a discouraged spelling in documentation prose or user-facing string literals;
- a hosted or paid service dependency in a manifest on the mandatory runtime path;
- a remote font, analytics or content delivery reference in the interface;
- a broken or placeholder public media reference;
- a tracked file that records a real account home directory, rather than a documented placeholder;
- a tracked text file that carries session token, model provider key, cloud key, signed web token, private key or runtime descriptor material;
- a tracked or ignored internal working notes file;
- a tracked copy of a private working document such as `spec.md` or `CLAUDE.md`;
- a commit whose author or committer is not the required identity, or whose message carries assistant, model or tool attribution.

## Comment detection is lexical, not regular

Comments are found by a hand-written lexer per language family rather than by pattern matching, because a regular expression cannot tell a comment from the same characters inside a string. The lexer understands Rust raw strings, byte strings, character literals and lifetimes, TypeScript template literals with interpolation and regular expression literals, CSS block comments, and shell quoting with a permitted shebang on the first line. It returns both the comments and the string literals, and the spelling rule uses the literals so that identifiers are never flagged.

## Documented exemptions

These exemptions are configured centrally rather than with inline suppressions.

- The dictionary file at `crates/policy/dictionary.toml` is never scanned by the spelling rule, because it necessarily contains the discouraged spellings it detects.
- The dictionary carries a list of protocol identifiers such as HTTP header names. A literal that is exactly one of them, or that begins with one followed by a colon, is exempt, because renaming a third-party API identifier would be incorrect.
- A string literal that looks like a regular expression is exempt from the spelling rule, because a pattern is not prose.
- The leakage rule never scans its own source at `crates/policy/src/rules/leakage.rs`, because that file necessarily contains the path prefixes and token shapes it detects.
- The leakage rule discriminates a real secret from a pattern definition or a documented sample by structure rather than by an exemption list. A candidate value must have at least eight distinct characters, must not be a monotonic character run, and must not contain a documented sample marker such as `EXAMPLE`. A private key is reported only when a well-formed header line is followed by enough encoded body to be real key material, so a regular expression that merely describes the header is never reported.

Generated files, the embedded asset directory, the documentation media and the fixtures are excluded from the first-party source rules.

Two lint configurations are also relaxed centrally with a reason. Test and fixture modules in the interface may export helpers alongside components, because the fast refresh rule does not apply to files that are never rendered by the development server. Browser test fixtures may use an empty destructuring pattern, because the browser test framework requires that exact signature.

## Spelling scope

The spelling rule reads documentation prose outside code fences and inline code, and string literals in first-party source. It compares whole tokens, treating hyphen and underscore as word characters, so a class name such as `items-center` is never reported.
