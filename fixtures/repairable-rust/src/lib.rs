pub fn banded_tariff(units: u32) -> u32 {
    match units {
        0 => 0,
        1..=100 => units * 12,
        101..=500 => 100 * 12 + (units - 100) * 9,
        _ => 100 * 12 + 400 * 9 + (units - 500) * 6,
    }
}

pub fn discounted_tariff(units: u32, discount_percent: u32) -> u32 {
    let gross = banded_tariff(units);
    gross - (gross * discount_percent / 100)
}

#[cfg(test)]
mod tariff_tests {
    use super::*;

    #[test]
    fn charges_the_first_band() {
        assert_eq!(banded_tariff(50), 600);
    }

    #[test]
    fn charges_across_bands() {
        assert_eq!(banded_tariff(300), 3000);
    }

    #[test]
    fn rounds_the_discount_half_away_from_zero() {
        assert_eq!(discounted_tariff(50, 15), 510);
    }
}
