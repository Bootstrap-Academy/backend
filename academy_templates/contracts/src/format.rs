//! German number formatting for the documents the platform issues.
//!
//! Invoices, credit notes, final statements and the confirmation emails are
//! written in German, so every number they contain has to be printed the way
//! the web interface prints it: `5,00 €`, `19 %`, `1.337`. Serializing a
//! [`Decimal`] straight into the template context instead produces the Rust
//! default (`5.00`, `19`), which is what the documents used to show.
//!
//! Every template context therefore serializes its numbers through the
//! functions below, so all of them are formatted in exactly one place.

use rust_decimal::Decimal;
use serde::Serializer;

/// Currency symbol every amount is printed with.
const CURRENCY: &str = "€";
/// Decimal places used for an amount of money.
const AMOUNT_DECIMALS: u32 = 2;
/// Decimal places used for the price of a single unit.
///
/// One Morphcoin costs a hundredth of a Euro gross, so its net price needs
/// more than two decimal places to multiply out to the net total of the line.
const UNIT_PRICE_DECIMALS: u32 = 4;
/// Separator between the whole and the fractional part.
const DECIMAL_SEPARATOR: char = ',';
/// Separator between two groups of three digits.
const GROUP_SEPARATOR: char = '.';

/// Format an amount of money, e.g. `1.234,50 €`.
pub fn amount(value: Decimal) -> String {
    format!("{} {CURRENCY}", fixed(value, AMOUNT_DECIMALS))
}

/// Format the price of a single unit, e.g. `0,0084 €`.
pub fn unit_price(value: Decimal) -> String {
    format!("{} {CURRENCY}", fixed(value, UNIT_PRICE_DECIMALS))
}

/// Format a percentage, e.g. `19 %`.
pub fn percent(value: Decimal) -> String {
    format!("{} %", significant(value))
}

/// Format a number of items, e.g. `1.337`.
pub fn count(value: u64) -> String {
    group(&value.to_string())
}

/// Format `value` with exactly `decimals` decimal places.
fn fixed(value: Decimal, decimals: u32) -> String {
    german(&format!(
        "{:.*}",
        decimals as usize,
        value.round_dp(decimals)
    ))
}

/// Format `value` with as few decimal places as it needs.
fn significant(value: Decimal) -> String {
    german(&value.normalize().to_string())
}

/// Turn the Rust representation of a number into the German one.
fn german(value: &str) -> String {
    let (sign, digits) = match value.strip_prefix('-') {
        Some(digits) => ("-", digits),
        None => ("", value),
    };
    let (integer, fraction) = match digits.split_once('.') {
        Some((integer, fraction)) => (integer, Some(fraction)),
        None => (digits, None),
    };

    let mut out = String::from(sign);
    out.push_str(&group(integer));
    if let Some(fraction) = fraction {
        out.push(DECIMAL_SEPARATOR);
        out.push_str(fraction);
    }
    out
}

/// Insert a group separator between every group of three digits.
fn group(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.char_indices() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(GROUP_SEPARATOR);
        }
        out.push(digit);
    }
    out
}

macro_rules! serializers {
    ($( $ident:ident($ty:ty, $format:path) ),* $(,)?) => { $(
        pub fn $ident<S: Serializer>(value: &$ty, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_str(&$format(*value))
        }
    )* };
}
serializers! {
    serialize_amount(Decimal, amount),
    serialize_unit_price(Decimal, unit_price),
    serialize_percent(Decimal, percent),
    serialize_count(u64, count),
}

#[cfg(test)]
mod tests {
    use rust_decimal_macros::dec;

    use super::*;

    #[test]
    fn amounts() {
        assert_eq!(amount(dec!(5)), "5,00 €");
        assert_eq!(amount(dec!(0.8)), "0,80 €");
        assert_eq!(amount(dec!(4.20168)), "4,20 €");
        assert_eq!(amount(dec!(13.37)), "13,37 €");
        assert_eq!(amount(dec!(1234.5)), "1.234,50 €");
        assert_eq!(amount(dec!(10000)), "10.000,00 €");
        assert_eq!(amount(dec!(1234567.891)), "1.234.567,89 €");
        assert_eq!(amount(dec!(0)), "0,00 €");
    }

    #[test]
    fn unit_prices() {
        assert_eq!(unit_price(dec!(0.0084033613445378151260504202)), "0,0084 €");
        assert_eq!(unit_price(dec!(1)), "1,0000 €");
    }

    /// The net unit price has to multiply out to the net total of the line,
    /// which two decimal places could not do for a Morphcoin.
    #[test]
    fn unit_price_multiplies_out() {
        let net_unit = Decimal::ONE / dec!(100) / dec!(1.19);
        assert_eq!(unit_price(net_unit), "0,0084 €");
        assert_eq!(amount(net_unit.round_dp(4) * dec!(500)), "4,20 €");
    }

    #[test]
    fn percentages() {
        assert_eq!(percent(dec!(19)), "19 %");
        assert_eq!(percent(dec!(19.00)), "19 %");
        assert_eq!(percent(dec!(7.5)), "7,5 %");
        assert_eq!(percent(dec!(0)), "0 %");
    }

    #[test]
    fn counts() {
        assert_eq!(count(0), "0");
        assert_eq!(count(500), "500");
        assert_eq!(count(1337), "1.337");
        assert_eq!(count(1000000), "1.000.000");
        assert_eq!(count(u64::MAX), "18.446.744.073.709.551.615");
    }

    #[test]
    fn negative_amounts() {
        assert_eq!(amount(dec!(-1234.5)), "-1.234,50 €");
    }
}
