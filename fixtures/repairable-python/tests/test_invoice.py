import sys
from decimal import Decimal
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from invoice import invoice_total, line_total, round_currency


def test_round_currency_rounds_half_away_from_zero():
    assert round_currency("0.125") == Decimal("0.13")
    assert round_currency("0.135") == Decimal("0.14")
    assert round_currency("-0.125") == Decimal("-0.13")


def test_line_total_multiplies_and_rounds():
    assert line_total("1.005", 1) == Decimal("1.01")
    assert line_total("2.50", 4) == Decimal("10.00")


def test_invoice_total_sums_rounded_line_totals():
    assert invoice_total([("1.005", 1), ("2.005", 1)]) == Decimal("3.02")
    assert invoice_total([]) == Decimal("0.00")
