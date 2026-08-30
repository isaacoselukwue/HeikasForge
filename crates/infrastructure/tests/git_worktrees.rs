use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;

use heikas_application::ports::git::{CommitRequest, GitService};
use heikas_application::ports::process::ProcessRunner;
use heikas_domain::identity::{BranchName, CandidateId, CandidateOrdinal, RunId};
use heikas_domain::path_policy::WorktreeRole;
use heikas_infrastructure::git::CommandLineGitService;
use heikas_infrastructure::layout::StoreLayout;
use heikas_infrastructure::process::SupervisedProcessRunner;
use tempfile::TempDir;

const AUTHOR: &str = "Isaac Oselukwue";
const EMAIL: &str = "fixture@localhost.invalid";

struct Harness {
    _home: TempDir,
    _repository_root: TempDir,
    repository: std::path::PathBuf,
    service: CommandLineGitService,
}

fn git(directory: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .env("GIT_AUTHOR_NAME", AUTHOR)
        .env("GIT_AUTHOR_EMAIL", EMAIL)
        .env("GIT_COMMITTER_NAME", AUTHOR)
        .env("GIT_COMMITTER_EMAIL", EMAIL)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {arguments:?} failed");
}

fn harness() -> Harness {
    let home = TempDir::new().expect("a temporary home");
    let repository_root = TempDir::new().expect("a temporary repository");
    let repository = repository_root.path().to_path_buf();
    std::fs::write(repository.join("value.txt"), "one\n").expect("the file writes");
    std::fs::create_dir_all(repository.join("src")).expect("the directory creates");
    std::fs::write(repository.join("src").join("main.txt"), "alpha\n").expect("the file writes");
    git(&repository, &["init", "--quiet"]);
    git(&repository, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    git(&repository, &["config", "user.name", AUTHOR]);
    git(&repository, &["config", "user.email", EMAIL]);
    git(&repository, &["config", "commit.gpgsign", "false"]);
    git(&repository, &["add", "-A"]);
    git(
        &repository,
        &["commit", "--quiet", "--message", "Add the initial content"],
    );

    let layout = StoreLayout::new(home.path().to_path_buf());
    let processes: Arc<dyn ProcessRunner> = Arc::new(SupervisedProcessRunner::new(Vec::new()));
    let service = CommandLineGitService::new(processes, layout, AUTHOR.to_string());
    Harness {
        _home: home,
        _repository_root: repository_root,
        repository,
        service,
    }
}

#[tokio::test]
async fn a_clean_repository_reports_its_baseline_and_identity() {
    let harness = harness();
    let facts = harness
        .service
        .inspect(&harness.repository)
        .await
        .expect("the repository inspects");
    assert!(facts.is_clean);
    assert_eq!(facts.default_branch, "main");
    assert_eq!(facts.configured_user_name.as_deref(), Some(AUTHOR));
    assert_eq!(facts.configured_user_email.as_deref(), Some(EMAIL));
    assert!(!facts.has_submodules);
    assert_eq!(facts.head_commit.as_str().len(), 40);
}

#[tokio::test]
async fn a_dirty_repository_reports_every_change_class() {
    let harness = harness();
    std::fs::write(harness.repository.join("value.txt"), "two\n").expect("the file writes");
    std::fs::write(harness.repository.join("untracked.txt"), "new\n").expect("the file writes");
    git(&harness.repository, &["add", "value.txt"]);
    std::fs::write(harness.repository.join("value.txt"), "three\n").expect("the file writes");

    let facts = harness
        .service
        .inspect(&harness.repository)
        .await
        .expect("the repository inspects");
    assert!(!facts.is_clean);
    assert!(facts.staged_paths.contains(&"value.txt".to_string()));
    assert!(facts.unstaged_paths.contains(&"value.txt".to_string()));
    assert!(facts.untracked_paths.contains(&"untracked.txt".to_string()));
}

#[tokio::test]
async fn a_dirty_snapshot_captures_tracked_and_untracked_changes() {
    let harness = harness();
    std::fs::write(harness.repository.join("value.txt"), "changed\n").expect("the file writes");
    std::fs::write(harness.repository.join("extra.txt"), "brand new\n").expect("the file writes");

    let snapshot = harness
        .service
        .capture_dirty_snapshot(&harness.repository)
        .await
        .expect("the snapshot captures");
    assert!(!snapshot.tracked_patch.is_empty());
    assert!(!snapshot.untracked_archive.is_empty());
    assert!(snapshot.untracked_paths.contains(&"extra.txt".to_string()));

    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    let candidate = CandidateId::derive(run, CandidateOrdinal::new(1).expect("an ordinal"));
    let facts = harness
        .service
        .inspect(&harness.repository)
        .await
        .expect("inspect");
    let branch = BranchName::from_str("heikas/work/snapshot").expect("a branch name");
    let handle = harness
        .service
        .create_worktree(
            &harness.repository,
            run,
            Some(&candidate),
            WorktreeRole::Candidate,
            &facts.head_commit,
            &branch,
        )
        .await
        .expect("the worktree creates");
    harness
        .service
        .apply_snapshot(&handle.path, &snapshot)
        .await
        .expect("the snapshot applies");
    assert_eq!(
        std::fs::read_to_string(handle.path.join("value.txt")).expect("the file reads"),
        "changed\n"
    );
    assert_eq!(
        std::fs::read_to_string(handle.path.join("extra.txt")).expect("the file reads"),
        "brand new\n"
    );
}

#[tokio::test]
async fn candidate_worktrees_are_isolated_and_diff_independently() {
    let harness = harness();
    let facts = harness
        .service
        .inspect(&harness.repository)
        .await
        .expect("inspect");
    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    let mut worktrees = Vec::new();
    for ordinal in 1..=3u8 {
        let candidate =
            CandidateId::derive(run, CandidateOrdinal::new(ordinal).expect("an ordinal"));
        let branch = BranchName::from_str(&format!("heikas/work/c{ordinal:02}")).expect("a branch");
        let handle = harness
            .service
            .create_worktree(
                &harness.repository,
                run,
                Some(&candidate),
                WorktreeRole::Candidate,
                &facts.head_commit,
                &branch,
            )
            .await
            .expect("the worktree creates");
        std::fs::write(
            handle.path.join("value.txt"),
            format!("candidate {ordinal}\n"),
        )
        .expect("the file writes");
        worktrees.push(handle);
    }

    for (index, handle) in worktrees.iter().enumerate() {
        let content =
            std::fs::read_to_string(handle.path.join("value.txt")).expect("the file reads");
        assert_eq!(content, format!("candidate {}\n", index + 1));
        let (patch, summary) = harness
            .service
            .diff_against_baseline(&handle.path, &facts.head_commit)
            .await
            .expect("the diff computes");
        assert!(!summary.is_empty);
        assert_eq!(summary.changed_files, 1);
        assert!(String::from_utf8_lossy(&patch).contains(&format!("candidate {}", index + 1)));
    }

    let source =
        std::fs::read_to_string(harness.repository.join("value.txt")).expect("the file reads");
    assert_eq!(source, "one\n", "the source worktree must remain untouched");
}

#[tokio::test]
async fn an_existing_worktree_is_reused_rather_than_recreated() {
    let harness = harness();
    let facts = harness
        .service
        .inspect(&harness.repository)
        .await
        .expect("inspect");
    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    let candidate = CandidateId::derive(run, CandidateOrdinal::new(1).expect("an ordinal"));
    let branch = BranchName::from_str("heikas/work/reuse").expect("a branch");
    let first = harness
        .service
        .create_worktree(
            &harness.repository,
            run,
            Some(&candidate),
            WorktreeRole::Candidate,
            &facts.head_commit,
            &branch,
        )
        .await
        .expect("the worktree creates");
    assert!(!first.reused);
    std::fs::write(first.path.join("marker.txt"), "kept\n").expect("the file writes");

    let second = harness
        .service
        .create_worktree(
            &harness.repository,
            run,
            Some(&candidate),
            WorktreeRole::Candidate,
            &facts.head_commit,
            &branch,
        )
        .await
        .expect("the worktree is reused");
    assert!(second.reused);
    assert_eq!(first.path, second.path);
    assert!(second.path.join("marker.txt").exists());
}

#[tokio::test]
async fn a_patch_applies_to_a_clean_integration_worktree_and_conflicts_are_detected() {
    let harness = harness();
    let facts = harness
        .service
        .inspect(&harness.repository)
        .await
        .expect("inspect");
    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    let candidate = CandidateId::derive(run, CandidateOrdinal::new(1).expect("an ordinal"));

    let candidate_branch = BranchName::from_str("heikas/work/patch").expect("a branch");
    let candidate_worktree = harness
        .service
        .create_worktree(
            &harness.repository,
            run,
            Some(&candidate),
            WorktreeRole::Candidate,
            &facts.head_commit,
            &candidate_branch,
        )
        .await
        .expect("the worktree creates");
    std::fs::write(candidate_worktree.path.join("value.txt"), "patched\n")
        .expect("the file writes");
    let (patch, _summary) = harness
        .service
        .diff_against_baseline(&candidate_worktree.path, &facts.head_commit)
        .await
        .expect("the diff computes");

    let integration_branch = BranchName::from_str("heikas/integration/patch").expect("a branch");
    let integration = harness
        .service
        .create_worktree(
            &harness.repository,
            run,
            None,
            WorktreeRole::Integration,
            &facts.head_commit,
            &integration_branch,
        )
        .await
        .expect("the integration worktree creates");

    assert!(harness
        .service
        .check_patch_applies(&integration.path, &patch)
        .await
        .expect("the check runs")
        .is_ok());
    harness
        .service
        .apply_patch(&integration.path, &patch)
        .await
        .expect("the patch applies");
    assert_eq!(
        std::fs::read_to_string(integration.path.join("value.txt")).expect("the file reads"),
        "patched\n"
    );

    let conflicting = harness
        .service
        .check_patch_applies(&integration.path, &patch)
        .await
        .expect("the check runs");
    assert!(
        conflicting.is_err(),
        "reapplying the same patch must be reported as a conflict"
    );
}

#[tokio::test]
async fn resetting_an_integration_worktree_restores_the_baseline() {
    let harness = harness();
    let facts = harness
        .service
        .inspect(&harness.repository)
        .await
        .expect("inspect");
    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    let branch = BranchName::from_str("heikas/integration/reset").expect("a branch");
    let integration = harness
        .service
        .create_worktree(
            &harness.repository,
            run,
            None,
            WorktreeRole::Integration,
            &facts.head_commit,
            &branch,
        )
        .await
        .expect("the worktree creates");
    std::fs::write(integration.path.join("value.txt"), "dirty\n").expect("the file writes");
    std::fs::write(integration.path.join("stray.txt"), "stray\n").expect("the file writes");

    harness
        .service
        .reset_worktree(&integration.path, &facts.head_commit)
        .await
        .expect("the worktree resets");
    assert_eq!(
        std::fs::read_to_string(integration.path.join("value.txt")).expect("the file reads"),
        "one\n"
    );
    assert!(!integration.path.join("stray.txt").exists());
}

#[tokio::test]
async fn a_commit_uses_the_required_identity_and_never_touches_the_default_branch() {
    let harness = harness();
    let facts = harness
        .service
        .inspect(&harness.repository)
        .await
        .expect("inspect");
    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    let integration_branch = BranchName::from_str("heikas/integration/commit").expect("a branch");
    let integration = harness
        .service
        .create_worktree(
            &harness.repository,
            run,
            None,
            WorktreeRole::Integration,
            &facts.head_commit,
            &integration_branch,
        )
        .await
        .expect("the worktree creates");
    std::fs::write(integration.path.join("value.txt"), "final\n").expect("the file writes");

    let output_branch = BranchName::from_str("heikas/run-abcdef01").expect("a branch");
    let outcome = harness
        .service
        .create_commit(&CommitRequest {
            worktree: integration.path.clone(),
            branch: output_branch.clone(),
            paths: vec!["value.txt".to_string()],
            subject: "Change the recorded value".to_string(),
            body: "Validated changes:\n- value.txt\n".to_string(),
            author_name: AUTHOR.to_string(),
            committer_name: AUTHOR.to_string(),
        })
        .await
        .expect("the commit creates");
    assert_eq!(outcome.branch, output_branch);
    assert_eq!(outcome.changed_files, 1);

    let inspected = Command::new("git")
        .args([
            "log",
            "-1",
            "--format=%an%x1f%ae%x1f%cn%x1f%ce%x1f%s",
            outcome.commit_hash.as_str(),
        ])
        .current_dir(&harness.repository)
        .output()
        .expect("git runs");
    let text = String::from_utf8_lossy(&inspected.stdout);
    let fields: Vec<&str> = text.trim().split('\u{1f}').collect();
    assert_eq!(fields[0], AUTHOR);
    assert_eq!(fields[1], EMAIL);
    assert_eq!(fields[2], AUTHOR);
    assert_eq!(fields[3], EMAIL);
    assert_eq!(fields[4], "Change the recorded value");

    let main_value = Command::new("git")
        .args(["show", "main:value.txt"])
        .current_dir(&harness.repository)
        .output()
        .expect("git runs");
    assert_eq!(
        String::from_utf8_lossy(&main_value.stdout),
        "one\n",
        "the default branch must never be modified"
    );
}

#[tokio::test]
async fn a_file_can_be_read_at_the_baseline_commit() {
    let harness = harness();
    let facts = harness
        .service
        .inspect(&harness.repository)
        .await
        .expect("inspect");
    let content = harness
        .service
        .file_at_commit(&harness.repository, &facts.head_commit, "src/main.txt")
        .await
        .expect("the read runs")
        .expect("the file exists at the baseline");
    assert_eq!(String::from_utf8_lossy(&content), "alpha\n");

    let absent = harness
        .service
        .file_at_commit(&harness.repository, &facts.head_commit, "src/absent.txt")
        .await
        .expect("the read runs");
    assert!(absent.is_none());
}

#[tokio::test]
async fn a_removed_worktree_is_pruned_from_the_repository() {
    let harness = harness();
    let facts = harness
        .service
        .inspect(&harness.repository)
        .await
        .expect("inspect");
    let run = RunId::from_uuid(uuid::Uuid::now_v7());
    let candidate = CandidateId::derive(run, CandidateOrdinal::new(1).expect("an ordinal"));
    let branch = BranchName::from_str("heikas/work/removable").expect("a branch");
    let handle = harness
        .service
        .create_worktree(
            &harness.repository,
            run,
            Some(&candidate),
            WorktreeRole::Candidate,
            &facts.head_commit,
            &branch,
        )
        .await
        .expect("the worktree creates");

    let listed = harness
        .service
        .list_run_worktrees(&harness.repository, run)
        .await
        .expect("the listing runs");
    assert!(!listed.is_empty());

    harness
        .service
        .remove_worktree(&harness.repository, &handle.path)
        .await
        .expect("the worktree removes");
    assert!(!handle.path.exists());
    let after = harness
        .service
        .list_run_worktrees(&harness.repository, run)
        .await
        .expect("the listing runs");
    assert!(after.is_empty());
}

#[tokio::test]
async fn a_directory_outside_a_git_working_tree_is_rejected() {
    let harness = harness();
    let outside = TempDir::new().expect("a temporary directory");
    let outcome = harness.service.inspect(outside.path()).await;
    assert!(outcome.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn a_dirty_snapshot_never_follows_a_symbolic_link_out_of_the_repository() {
    let harness = harness();
    let outside = tempfile::TempDir::new().expect("a directory outside the repository");
    let secret = outside.path().join("id_rsa");
    std::fs::write(&secret, "PRIVATE MATERIAL\n").expect("the secret writes");
    std::os::unix::fs::symlink(&secret, harness.repository.join("notes.txt"))
        .expect("the link creates");

    let snapshot = harness
        .service
        .capture_dirty_snapshot(&harness.repository)
        .await
        .expect("the snapshot captures");

    let cursor = std::io::Cursor::new(snapshot.untracked_archive.clone());
    let mut archive = zip::ZipArchive::new(cursor).expect("the archive reads");
    let mut combined = String::new();
    for index in 0..archive.len() {
        use std::io::Read;
        let mut entry = archive.by_index(index).expect("an entry reads");
        let mut contents = String::new();
        let _ = entry.read_to_string(&mut contents);
        combined.push_str(&contents);
    }
    assert!(
        !combined.contains("PRIVATE MATERIAL"),
        "a symbolic link must never be dereferenced into the candidate worktrees"
    );
}
