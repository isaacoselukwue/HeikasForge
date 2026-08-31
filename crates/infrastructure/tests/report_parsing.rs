use heikas_infrastructure::quality::reports::{
    parse_cargo_test_summary, parse_go_test_json, parse_pytest_summary,
};

#[test]
fn the_cargo_summary_sums_every_test_binary() {
    let stdout = concat!(
        "test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s\n",
        "test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n",
    );
    let summary = parse_cargo_test_summary(stdout);
    assert_eq!(summary.total, 8);
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.skipped, 1);
}

#[test]
fn the_cargo_summary_reports_a_failure_and_an_empty_suite() {
    let failed = parse_cargo_test_summary(
        "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n",
    );
    assert_eq!(failed.total, 1);
    assert_eq!(failed.failed, 1);

    let empty = parse_cargo_test_summary(
        "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n",
    );
    assert_eq!(
        empty.total, 0,
        "a crate with no tests must report no executed tests"
    );
}

#[test]
fn the_pytest_summary_reads_each_reported_shape() {
    let passed = parse_pytest_summary("..s\n2 passed, 1 skipped in 0.05s\n");
    assert_eq!(passed.total, 3);
    assert_eq!(passed.failed, 0);
    assert_eq!(passed.skipped, 1);

    let failed = parse_pytest_summary(
        "=========================== short test summary info ============================\nFAILED tests/test_x.py::test_a - assert 1 == 2\n1 failed in 0.05s\n",
    );
    assert_eq!(failed.total, 1);
    assert_eq!(failed.failed, 1);

    let none = parse_pytest_summary("\nno tests ran in 0.00s\n");
    assert_eq!(
        none.total, 0,
        "a suite that collected nothing must report no executed tests"
    );
}

#[test]
fn the_go_event_stream_counts_tests_and_keeps_a_failure_message() {
    let stdout = concat!(
        r#"{"Action":"run","Package":"example/a","Test":"TestOne"}"#,
        "\n",
        r#"{"Action":"pass","Package":"example/a","Test":"TestOne","Elapsed":0.01}"#,
        "\n",
        r#"{"Action":"output","Package":"example/a","Test":"TestTwo","Output":"    x_test.go:9: boom\n"}"#,
        "\n",
        r#"{"Action":"fail","Package":"example/a","Test":"TestTwo","Elapsed":0.01}"#,
        "\n",
        r#"{"Action":"skip","Package":"example/a","Test":"TestThree","Elapsed":0}"#,
        "\n",
        r#"{"Action":"pass","Package":"example/a","Elapsed":0.02}"#,
        "\n",
    );
    let summary = parse_go_test_json(stdout);
    assert_eq!(
        summary.total, 3,
        "a package level event must not be counted"
    );
    assert_eq!(summary.failed, 1);
    assert_eq!(summary.skipped, 1);
    assert_eq!(summary.failures.len(), 1);
    assert!(summary.failures[0].message.contains("boom"));
    assert_eq!(summary.failures[0].case, "TestTwo");
}

#[test]
fn a_package_with_no_test_files_reports_no_executed_tests() {
    let stdout = concat!(
        r#"{"Action":"output","Package":"example/a","Output":"?   \texample/a\t[no test files]\n"}"#,
        "\n",
        r#"{"Action":"skip","Package":"example/a","Elapsed":0}"#,
        "\n",
    );
    assert_eq!(parse_go_test_json(stdout).total, 0);
}

#[test]
fn a_suite_in_which_everything_is_skipped_reports_no_executed_tests() {
    let ignored = parse_cargo_test_summary(
        "test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s\n",
    );
    assert_eq!(ignored.total, 2);
    assert_eq!(
        ignored.total - ignored.skipped,
        0,
        "an ignored test has not been executed, so it is not evidence"
    );

    let skipped = parse_pytest_summary("ss\n2 skipped in 0.01s\n");
    assert_eq!(skipped.total - skipped.skipped, 0);

    let go = parse_go_test_json(concat!(
        r#"{"Action":"skip","Package":"a","Test":"TestOne"}"#,
        "\n",
        r#"{"Action":"skip","Package":"a","Test":"TestTwo"}"#,
        "\n",
    ));
    assert_eq!(go.total - go.skipped, 0);
}
