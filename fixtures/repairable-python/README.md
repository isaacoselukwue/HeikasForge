# Repairable invoice fixture

A deliberately small Python package with one rounding defect and a matching test suite.

`src/invoice.py` rounds currency with banker's rounding while `tests/test_invoice.py` requires rounding half away from zero. The defect is real, so the configured test gate genuinely fails until a candidate repairs it.

`scripts/gate.py` provides every configured quality command and writes real JUnit XML, LCOV and SARIF reports.
