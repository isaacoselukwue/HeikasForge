use std::sync::Arc;

use heikas_application::ports::agent::ToolPolicy;
use heikas_application::ports::observability::Redactor;
use heikas_application::ports::process::ProcessRunner;
use heikas_domain::clock::TimeoutSeconds;
use heikas_domain::command::{CommandId, CommandKind, CommandSpecification, ReportFormat};
use heikas_domain::path_policy::PathPolicy;
use heikas_infrastructure::agent::tools::{ToolExecutor, COMPLETION_TOOL};
use heikas_infrastructure::process::SupervisedProcessRunner;
use heikas_infrastructure::redaction::{PatternRedactor, REDACTION_PLACEHOLDER};
use serde_json::json;
use std::str::FromStr;
use tempfile::TempDir;
use tokio::sync::watch;

fn worktree() -> TempDir {
    let directory = TempDir::new().expect("a temporary worktree");
    std::fs::create_dir_all(directory.path().join("src")).expect("the directory creates");
    std::fs::create_dir_all(directory.path().join(".git")).expect("the directory creates");
    std::fs::write(
        directory.path().join("src").join("main.rs"),
        "fn main() {}\n",
    )
    .expect("the file writes");
    std::fs::write(directory.path().join(".git").join("config"), "[core]\n")
        .expect("the file writes");
    std::fs::write(directory.path().join(".env"), "TOKEN=secret\n").expect("the file writes");
    directory
}

fn executor(
    directory: &TempDir,
    policy: ToolPolicy,
    commands: Vec<CommandSpecification>,
) -> ToolExecutor {
    let (_sender, receiver) = watch::channel(false);
    let processes: Arc<dyn ProcessRunner> = Arc::new(SupervisedProcessRunner::new(Vec::new()));
    ToolExecutor::new(
        directory.path().to_path_buf(),
        policy,
        commands,
        processes,
        receiver,
        65_536,
    )
}

fn named_command(id: &str, program: &str, arguments: &[&str]) -> CommandSpecification {
    CommandSpecification {
        id: CommandId::from_str(id).expect("a command identifier"),
        kind: CommandKind::Test,
        program: program.to_string(),
        args: arguments.iter().map(|value| (*value).to_string()).collect(),
        working_subdirectory: None,
        timeout: TimeoutSeconds::clamped(30, 600),
        required: true,
        report_format: ReportFormat::None,
        report_path: None,
        environment: Vec::new(),
        success_exit_codes: vec![0],
    }
}

#[tokio::test]
async fn a_read_only_policy_refuses_every_writing_tool() {
    let directory = worktree();
    let executor = executor(
        &directory,
        ToolPolicy::read_only(PathPolicy::default(), 40),
        Vec::new(),
    );
    for tool in [
        "write_file",
        "delete_file",
        "apply_patch",
        "run_named_command",
    ] {
        let outcome = executor
            .execute(tool, &json!({"path": "src/main.rs", "contents": "x", "patch": "x", "command_id": "test"}))
            .await
            .expect("the tool responds");
        assert!(
            !outcome.accepted,
            "{tool} must be refused for a read-only role"
        );
    }
}

#[tokio::test]
async fn a_read_only_policy_permits_inspection() {
    let directory = worktree();
    let executor = executor(
        &directory,
        ToolPolicy::read_only(PathPolicy::default(), 40),
        Vec::new(),
    );
    let listing = executor
        .execute("list_entries", &json!({"path": "."}))
        .await
        .expect("the tool responds");
    assert!(listing.accepted);
    let read = executor
        .execute("read_file", &json!({"path": "src/main.rs"}))
        .await
        .expect("the tool responds");
    assert!(read.accepted);
    assert!(read.result["contents"]
        .as_str()
        .expect("contents")
        .contains("fn main"));
}

#[tokio::test]
async fn a_path_that_escapes_the_worktree_is_refused() {
    let directory = worktree();
    let executor = executor(
        &directory,
        ToolPolicy::editing(PathPolicy::default(), Vec::new(), 40),
        Vec::new(),
    );
    for path in ["../escape.txt", "/etc/passwd", "src/../../escape.txt"] {
        let outcome = executor
            .execute("write_file", &json!({"path": path, "contents": "x"}))
            .await
            .expect("the tool responds");
        assert!(!outcome.accepted, "{path} must be refused");
    }
    assert!(!directory
        .path()
        .parent()
        .expect("a parent")
        .join("escape.txt")
        .exists());
}

#[tokio::test]
async fn a_symbolic_link_out_of_the_worktree_is_refused() {
    let directory = worktree();
    let outside = TempDir::new().expect("a temporary directory");
    std::fs::write(outside.path().join("target.txt"), "outside\n").expect("the file writes");
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path(), directory.path().join("link"))
        .expect("the link creates");
    #[cfg(not(unix))]
    return;

    let executor = executor(
        &directory,
        ToolPolicy::editing(PathPolicy::default(), Vec::new(), 40),
        Vec::new(),
    );
    let outcome = executor
        .execute(
            "write_file",
            &json!({"path": "link/target.txt", "contents": "changed"}),
        )
        .await
        .expect("the tool responds");
    assert!(
        !outcome.accepted,
        "a symbolic link out of the worktree must be refused"
    );
    assert_eq!(
        std::fs::read_to_string(outside.path().join("target.txt")).expect("the file reads"),
        "outside\n"
    );
}

#[tokio::test]
async fn a_protected_path_cannot_be_written_but_a_normal_path_can() {
    let directory = worktree();
    let executor = executor(
        &directory,
        ToolPolicy::editing(PathPolicy::default(), Vec::new(), 40),
        Vec::new(),
    );
    let protected = executor
        .execute(
            "write_file",
            &json!({"path": ".git/config", "contents": "x"}),
        )
        .await
        .expect("the tool responds");
    assert!(!protected.accepted);

    let permitted = executor
        .execute(
            "write_file",
            &json!({"path": "src/added.rs", "contents": "pub fn added() {}\n"}),
        )
        .await
        .expect("the tool responds");
    assert!(permitted.accepted);
    assert!(directory.path().join("src").join("added.rs").exists());
}

#[tokio::test]
async fn a_sensitive_path_cannot_even_be_read() {
    let directory = worktree();
    let executor = executor(
        &directory,
        ToolPolicy::editing(PathPolicy::default(), Vec::new(), 40),
        Vec::new(),
    );
    let outcome = executor
        .execute("read_file", &json!({"path": ".env"}))
        .await
        .expect("the tool responds");
    assert!(!outcome.accepted, "a sensitive path must not be readable");
}

#[tokio::test]
async fn a_patch_touching_a_protected_path_is_refused() {
    let directory = worktree();
    let executor = executor(
        &directory,
        ToolPolicy::editing(PathPolicy::default(), Vec::new(), 40),
        Vec::new(),
    );
    let patch = "--- a/.git/config\n+++ b/.git/config\n@@ -1 +1 @@\n-[core]\n+[core] tampered\n";
    let outcome = executor
        .execute("apply_patch", &json!({"patch": patch}))
        .await
        .expect("the tool responds");
    assert!(!outcome.accepted);
}

#[tokio::test]
async fn only_configured_named_commands_can_be_run() {
    let directory = worktree();
    let permitted = named_command(
        "test",
        "python3",
        &["-c", "print('ran the configured command')"],
    );
    let executor = executor(
        &directory,
        ToolPolicy::editing(PathPolicy::default(), vec![permitted.id.clone()], 40),
        vec![permitted],
    );

    let allowed = executor
        .execute("run_named_command", &json!({"command_id": "test"}))
        .await
        .expect("the tool responds");
    assert!(allowed.accepted);
    assert!(allowed.result["stdout"]
        .as_str()
        .expect("stdout")
        .contains("ran the configured command"));

    let refused = executor
        .execute("run_named_command", &json!({"command_id": "lint"}))
        .await
        .expect("the tool responds");
    assert!(!refused.accepted);
}

#[tokio::test]
async fn no_general_shell_tool_is_exposed() {
    let directory = worktree();
    let executor = executor(
        &directory,
        ToolPolicy::editing(PathPolicy::default(), Vec::new(), 40),
        Vec::new(),
    );
    let definitions = executor.definitions(&json!({"type": "object"}));
    let names: Vec<&str> = definitions
        .iter()
        .map(|definition| definition.name.as_str())
        .collect();
    for forbidden in ["shell", "bash", "sh", "exec", "run_command", "system"] {
        assert!(
            !names.contains(&forbidden),
            "{forbidden} must never be exposed"
        );
    }
    assert!(names.contains(&COMPLETION_TOOL));

    let outcome = executor
        .execute("shell", &json!({"command": "rm -rf /"}))
        .await
        .expect("the tool responds");
    assert!(!outcome.accepted);
}

#[tokio::test]
async fn a_file_beyond_the_read_limit_is_refused() {
    let directory = worktree();
    let policy = PathPolicy {
        maximum_read_bytes: 64,
        ..PathPolicy::default()
    };
    std::fs::write(directory.path().join("large.txt"), "x".repeat(4_096)).expect("the file writes");
    let executor = executor(&directory, ToolPolicy::read_only(policy, 40), Vec::new());
    let outcome = executor
        .execute("read_file", &json!({"path": "large.txt"}))
        .await
        .expect("the tool responds");
    assert!(!outcome.accepted);
}

#[test]
fn known_secret_shapes_are_redacted() {
    let redactor = PatternRedactor::without_environment();
    let samples = [
        "ghp_0123456789abcdefghijklmnopqrstuvwxyz",
        "sk-0123456789abcdefghijklmnop",
        "AKIAIOSFODNN7EXAMPLE",
        "https://user:supersecret@example.invalid/path",
        "Authorization: Bearer abcdefghijklmnopqrstuvwxyz012345",
    ];
    for sample in samples {
        let redacted = redactor.redact_text(sample);
        assert!(
            redacted.contains(REDACTION_PLACEHOLDER),
            "`{sample}` must be redacted, produced `{redacted}`"
        );
    }
}

#[test]
fn a_private_key_block_is_redacted_entirely() {
    let redactor = PatternRedactor::without_environment();
    let text = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\n-----END RSA PRIVATE KEY-----";
    let redacted = redactor.redact_text(text);
    assert!(!redacted.contains("MIIEowIBAAKCAQEA"));
    assert!(redacted.contains(REDACTION_PLACEHOLDER));
}

#[test]
fn a_configured_secret_value_is_redacted_from_output() {
    std::env::set_var("HEIKAS_TEST_TOKEN", "a-very-secret-value-1234");
    let redactor = PatternRedactor::new(
        &["HEIKAS_TEST_TOKEN".to_string()],
        &[],
        Some("/home/operator".to_string()),
    );
    let redacted =
        redactor.redact_text("the token a-very-secret-value-1234 is in /home/operator/work");
    assert!(!redacted.contains("a-very-secret-value-1234"));
    assert!(redacted.contains(REDACTION_PLACEHOLDER));
    assert!(
        redacted.contains("~/work"),
        "the home prefix must be replaced"
    );
}

#[test]
fn sensitive_json_keys_are_redacted_by_name() {
    let redactor = PatternRedactor::without_environment();
    let document = json!({
        "safe": "visible",
        "api_key": "abcdefghijklmnop",
        "nested": { "password": "hunter2", "note": "visible too" }
    });
    let redacted = redactor.redact_json(&document);
    assert_eq!(redacted["safe"], "visible");
    assert_eq!(redacted["api_key"], REDACTION_PLACEHOLDER);
    assert_eq!(redacted["nested"]["password"], REDACTION_PLACEHOLDER);
    assert_eq!(redacted["nested"]["note"], "visible too");
}

#[test]
fn redacting_binary_content_leaves_it_unchanged() {
    let redactor = PatternRedactor::without_environment();
    let bytes = vec![0u8, 159, 146, 150, 255];
    assert_eq!(redactor.redact_bytes(&bytes), bytes);
}
