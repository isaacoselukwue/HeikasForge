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

use heikas_infrastructure::quality::reports::{parse_ctest_summary, parse_node_test_summary};

#[test]
fn the_node_builtin_runner_summary_is_read() {
    let stdout = concat!(
        "1..3\n# tests 3\n# suites 0\n# pass 2\n# fail 0\n",
        "# cancelled 0\n# skipped 1\n# todo 0\n# duration_ms 49.2\n",
    );
    let summary = parse_node_test_summary(stdout);
    assert_eq!(summary.total, 3);
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.skipped, 1);
    assert_eq!(summary.total - summary.skipped, 2);
}

#[test]
fn the_vitest_summary_is_read_in_each_shape() {
    let mixed = parse_node_test_summary(
        " Test Files  1 passed (1)\n      Tests  2 passed | 1 skipped (3)\n",
    );
    assert_eq!(mixed.total, 3);
    assert_eq!(mixed.skipped, 1);
    assert_eq!(mixed.failed, 0);

    let plain = parse_node_test_summary("      Tests  3 passed (3)\n");
    assert_eq!(plain.total, 3);
    assert_eq!(plain.skipped, 0);

    let failing = parse_node_test_summary(
        " Test Files  1 failed (1)\n      Tests  1 failed | 1 passed (2)\n",
    );
    assert_eq!(failing.total, 2);
    assert_eq!(failing.failed, 1);
}

#[test]
fn the_jest_summary_is_read() {
    let summary = parse_node_test_summary(
        "Test Suites: 1 passed, 1 total\nTests:       1 skipped, 2 passed, 3 total\nSnapshots:   0 total\n",
    );
    assert_eq!(summary.total, 3);
    assert_eq!(summary.skipped, 1);
    assert_eq!(summary.total - summary.skipped, 2);
}

#[test]
fn the_mocha_summary_is_read() {
    let summary = parse_node_test_summary("  2 passing (3ms)\n  1 pending\n");
    assert_eq!(summary.total, 3);
    assert_eq!(summary.skipped, 1);

    let failing = parse_node_test_summary("  1 passing (3ms)\n  2 failing\n");
    assert_eq!(failing.total, 3);
    assert_eq!(failing.failed, 2);
}

#[test]
fn a_node_run_that_executed_nothing_yields_no_executed_tests() {
    let vitest = parse_node_test_summary("No test files found, exiting with code 1\n");
    assert_eq!(vitest.total, 0);

    let all_skipped = parse_node_test_summary("      Tests  3 skipped (3)\n");
    assert_eq!(all_skipped.total - all_skipped.skipped, 0);

    let unrecognised = parse_node_test_summary("some runner nobody has heard of finished\n");
    assert_eq!(
        unrecognised.total, 0,
        "an unrecognised summary must fail the gate rather than pass it"
    );
}

#[test]
fn the_ctest_summary_is_read() {
    let passing = parse_ctest_summary("100% tests passed, 0 tests failed out of 12\n");
    assert_eq!(passing.total, 12);
    assert_eq!(passing.failed, 0);

    let mixed = parse_ctest_summary("50% tests passed, 1 tests failed out of 2\n");
    assert_eq!(mixed.total, 2);
    assert_eq!(mixed.failed, 1);

    assert_eq!(parse_ctest_summary("Errors while running CTest\n").total, 0);
}
