import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "src"))

from oracle import decide_halting


def test_decides_a_halting_program():
    assert decide_halting("def main():\n    return 1\n", "") is True


def test_decides_a_non_halting_program():
    assert decide_halting("def main():\n    while True:\n        pass\n", "") is False
