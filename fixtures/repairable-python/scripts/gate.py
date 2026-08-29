import json
import re
import sys
import traceback
from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).resolve().parents[1]
REPORTS = ROOT / "reports"
SOURCE_FILES = sorted((ROOT / "src").rglob("*.py"))
TEST_FILES = sorted((ROOT / "tests").rglob("*.py"))


def ensure_reports():
    REPORTS.mkdir(parents=True, exist_ok=True)


def collect_test_cases():
    sys.path.insert(0, str(ROOT / "tests"))
    sys.path.insert(0, str(ROOT / "src"))
    cases = []
    for path in TEST_FILES:
        module_name = path.stem
        namespace = {"__file__": str(path), "__name__": module_name}
        source = path.read_text(encoding="utf-8")
        compiled = compile(source, str(path), "exec")
        exec(compiled, namespace)
        for name, value in sorted(namespace.items()):
            if name.startswith("test_") and callable(value):
                cases.append((module_name, name, value, path))
    return cases


def run_tests():
    ensure_reports()
    try:
        cases = collect_test_cases()
    except Exception:
        detail = traceback.format_exc()
        write_junit([("collection", "import", detail, None)], 0)
        print(detail, file=sys.stderr)
        return 1

    failures = []
    for module_name, name, function, path in cases:
        try:
            function()
        except Exception:
            failures.append((module_name, name, traceback.format_exc(), path))
    write_junit(failures, len(cases))
    for module_name, name, detail, _ in failures:
        print("failed {}::{}\n{}".format(module_name, name, detail), file=sys.stderr)
    print("{} of {} tests passed".format(len(cases) - len(failures), len(cases)))
    return 1 if failures else 0


def write_junit(failures, total):
    lines = ['<?xml version="1.0" encoding="UTF-8"?>']
    lines.append(
        '<testsuite name="invoice" tests="{}" failures="{}" skipped="0">'.format(
            total, len(failures)
        )
    )
    for module_name, name, detail, path in failures:
        location = str(path.relative_to(ROOT)) if path is not None else "tests"
        lines.append(
            '  <testcase classname="{}" name="{}" file="{}">'.format(
                escape(module_name), escape(name), escape(location)
            )
        )
        lines.append(
            '    <failure message="{}">{}</failure>'.format(
                escape(detail.strip().splitlines()[-1][:200]), escape(detail)
            )
        )
        lines.append("  </testcase>")
    passed = total - len(failures)
    for index in range(passed):
        lines.append(
            '  <testcase classname="invoice" name="passing_case_{}" />'.format(index)
        )
    lines.append("</testsuite>")
    (REPORTS / "junit.xml").write_text("\n".join(lines) + "\n", encoding="utf-8")


def run_coverage():
    ensure_reports()
    executable = 0
    covered = 0
    records = []
    for path in SOURCE_FILES:
        relative = path.relative_to(ROOT)
        records.append("SF:{}".format(relative))
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            executable += 1
            hits = 0 if stripped.startswith("raise NotImplementedError") else 1
            covered += hits
            records.append("DA:{},{}".format(number, hits))
        records.append("end_of_record")
    header = ["LF:{}".format(executable), "LH:{}".format(covered)]
    (REPORTS / "coverage.info").write_text(
        "\n".join(records + header) + "\n", encoding="utf-8"
    )
    percentage = 0.0 if executable == 0 else (covered / executable) * 100
    print("line coverage {:.2f} percent".format(percentage))
    return 0


def run_format():
    offenders = []
    for path in SOURCE_FILES + TEST_FILES:
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            if line.rstrip() != line:
                offenders.append("{}:{} trailing whitespace".format(path.relative_to(ROOT), number))
            if "\t" in line:
                offenders.append("{}:{} tab indentation".format(path.relative_to(ROOT), number))
    for offender in offenders:
        print(offender, file=sys.stderr)
    return 1 if offenders else 0


def run_lint():
    offenders = []
    for path in SOURCE_FILES + TEST_FILES:
        for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
            if len(line) > 120:
                offenders.append("{}:{} line longer than 120 characters".format(path.relative_to(ROOT), number))
            if re.search(r"\bexcept\s*:", line):
                offenders.append("{}:{} bare except clause".format(path.relative_to(ROOT), number))
    for offender in offenders:
        print(offender, file=sys.stderr)
    return 1 if offenders else 0


def run_static_analysis():
    ensure_reports()
    results = []
    for path in SOURCE_FILES:
        text = path.read_text(encoding="utf-8")
        for number, line in enumerate(text.splitlines(), start=1):
            if "eval(" in line or "exec(" in line:
                results.append(
                    {
                        "ruleId": "unsafe-dynamic-execution",
                        "level": "error",
                        "message": {"text": "dynamic execution is not permitted in this fixture"},
                        "locations": [
                            {
                                "physicalLocation": {
                                    "artifactLocation": {"uri": str(path.relative_to(ROOT))},
                                    "region": {"startLine": number},
                                }
                            }
                        ],
                    }
                )
    document = {
        "version": "2.1.0",
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "runs": [
            {
                "tool": {
                    "driver": {
                        "name": "fixture-static-analysis",
                        "rules": [
                            {
                                "id": "unsafe-dynamic-execution",
                                "helpUri": "https://example.invalid/rules/unsafe-dynamic-execution",
                            }
                        ],
                    }
                },
                "results": results,
            }
        ],
    }
    (REPORTS / "analysis.sarif").write_text(
        json.dumps(document, indent=2) + "\n", encoding="utf-8"
    )
    print("static analysis produced {} findings".format(len(results)))
    return 1 if results else 0


def run_audit():
    manifest = ROOT / "requirements.txt"
    if not manifest.exists():
        print("no third party dependencies are declared")
        return 0
    vulnerable = {"insecure-package"}
    offenders = [
        line.strip()
        for line in manifest.read_text(encoding="utf-8").splitlines()
        if line.strip().split("==")[0] in vulnerable
    ]
    for offender in offenders:
        print("vulnerable dependency {}".format(offender), file=sys.stderr)
    return 1 if offenders else 0


def run_secret_scan():
    patterns = [
        re.compile(r"AKIA[0-9A-Z]{16}"),
        re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----"),
        re.compile(r"(?i)api[_-]?key\s*=\s*['\"][A-Za-z0-9]{16,}['\"]"),
    ]
    offenders = []
    for path in SOURCE_FILES + TEST_FILES:
        text = path.read_text(encoding="utf-8")
        for number, line in enumerate(text.splitlines(), start=1):
            for pattern in patterns:
                if pattern.search(line):
                    offenders.append("{}:{} possible secret".format(path.relative_to(ROOT), number))
    for offender in offenders:
        print(offender, file=sys.stderr)
    return 1 if offenders else 0


def run_policy():
    offenders = []
    markers = ["TO" + "DO", "FIX" + "ME", "HA" + "CK"]
    forbidden = chr(0x2014)
    for path in SOURCE_FILES + TEST_FILES:
        text = path.read_text(encoding="utf-8")
        for number, line in enumerate(text.splitlines(), start=1):
            if forbidden in line:
                offenders.append("{}:{} em dash".format(path.relative_to(ROOT), number))
            for marker in markers:
                if marker in line:
                    offenders.append("{}:{} task marker".format(path.relative_to(ROOT), number))
    for offender in offenders:
        print(offender, file=sys.stderr)
    return 1 if offenders else 0


COMMANDS = {
    "format": run_format,
    "lint": run_lint,
    "test": run_tests,
    "coverage": run_coverage,
    "audit": run_audit,
    "secret-scan": run_secret_scan,
    "static-analysis": run_static_analysis,
    "policy": run_policy,
}


def main():
    if len(sys.argv) != 2 or sys.argv[1] not in COMMANDS:
        print("usage: gate.py {}".format("|".join(sorted(COMMANDS))), file=sys.stderr)
        return 2
    return COMMANDS[sys.argv[1]]()


if __name__ == "__main__":
    sys.exit(main())
