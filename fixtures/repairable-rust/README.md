# Repairable Rust fixture

A single crate with a banded tariff calculator. `discounted_tariff` truncates the discount instead of rounding half away from zero, so `discounted_tariff(50, 15)` returns 511 rather than 510. The defect is real and the bundled test proves it.
