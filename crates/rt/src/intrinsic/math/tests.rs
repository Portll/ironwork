use super::*;
use std::ops::{Add, Neg};
use zarch::hfp::{Hfp, Precision};

fn n(v: i128) -> Real {
    Real::from_i128(v)
}

fn half() -> Real {
    Real::ONE.scaled(-1)
}

/// `hex` is the sign-and-characteristic byte and then the fraction's hex digits, from a
/// 120-digit decimal oracle rounded to nearest.
fn hfp(hex: &str, precision: Precision) -> Hfp {
    let head = u8::from_str_radix(&hex[..2], 16).unwrap();
    Hfp { precision, negative: head & 0x80 != 0, characteristic: head & 0x7f, fraction: u128::from_str_radix(&hex[2..], 16).unwrap() }
}

fn check(name: &str, value: Real, long: &str, extended: &str) {
    assert_eq!(value.to_hfp(Precision::Long).unwrap(), hfp(long, Precision::Long), "{name} long");
    assert_eq!(value.to_hfp(Precision::Extended).unwrap(), hfp(extended, Precision::Extended), "{name} extended");
}

#[test]
fn every_function_rounds_to_the_nearest_long_and_extended_value() {
    let cases: Vec<(&str, Real, &str, &str)> = vec![
        ("sqrt(2)", sqrt(n(2)).unwrap(), "4116A09E667F3BCD", "4116A09E667F3BCC908B2FB1366EA9"),
        ("sqrt(0.5)", sqrt(half()).unwrap(), "40B504F333F9DE65", "40B504F333F9DE6484597D89B3754B"),
        ("sqrt(1e6+1)", sqrt(n(1_000_001)).unwrap(), "433E80020C49B1C7", "433E80020C49B1C72F93783DCF1859"),
        ("exp(1)", exp(n(1)), "412B7E151628AED3", "412B7E151628AED2A6ABF7158809CF"),
        ("exp(-1)", exp(n(-1)), "405E2D58D8B3BCDF", "405E2D58D8B3BCDF1ABADEC7829055"),
        ("exp(100)", exp(n(100)), "6513494A9B171BF5", "6513494A9B171BF4ACC22509332243"),
        ("exp(-0.25)", exp(half().scaled(-1).neg()), "40C75F7CF5641057", "40C75F7CF564105743415CBC9D6369"),
        ("ln(2)", ln(n(2)).unwrap(), "40B17217F7D1CF7A", "40B17217F7D1CF79ABC9E3B39803F3"),
        ("ln(10)", ln(n(10)).unwrap(), "4124D763776AAA2B", "4124D763776AAA2B05BA95B58AE0B5"),
        ("ln(0.75)", ln(n(3).scaled(-2)).unwrap(), "C049A58844D36E4A", "C049A58844D36E49E0EFADD9DB02AA"),
        ("ln(1e20)", ln(n(100_000_000_000_000_000_000)).unwrap(), "422E0D3C554554B6", "422E0D3C554554B5C7293B22ED98E2"),
        ("log10(2)", log10(n(2)).unwrap(), "404D104D427DE7FC", "404D104D427DE7FBCC47C4ACD605BE"),
        ("log10(1000)", log10(n(1000)).unwrap(), "4130000000000000", "413000000000000000000000000000"),
        ("exp10(0.5)", exp10(half()), "413298B075B4B6A5", "413298B075B4B6A5240945790619B3"),
        ("exp10(20)", exp10(n(20)), "5156BC75E2D63100", "5156BC75E2D6310000000000000000"),
        ("sin(1)", sin(n(1)).unwrap(), "40D76AA478486770", "40D76AA47848677020C6E9E909C50F"),
        ("sin(1e6)", sin(n(1_000_000)).unwrap(), "C059992C95A3619D", "C059992C95A3619D264732D26E9B95"),
        ("sin(-3)", sin(n(-3)).unwrap(), "C0242070DB6DAAB7", "C0242070DB6DAAB69E3902E8468315"),
        ("sin(355)", sin(n(355)).unwrap(), "BD1F9BD0307D1DE3", "BD1F9BD0307D1DE29DADE2CB6B5CB8"),
        ("cos(1)", cos(n(1)).unwrap(), "408A51407DA8345D", "408A51407DA8345C91C2466D976872"),
        ("cos(1e10)", cos(n(10_000_000_000)).unwrap(), "40DF84C480E498CC", "40DF84C480E498CC1933FBE189E152"),
        ("cos(0.5)", cos(half()).unwrap(), "40E0A94032DBEA7D", "40E0A94032DBEA7CEDBDDD9DA2FAFB"),
        ("tan(1)", tan(n(1)).unwrap(), "4118EB245CBEE3A6", "4118EB245CBEE3A5B8ACC7D4132314"),
        ("tan(-3)", tan(n(-3)).unwrap(), "40247DEE24A970DE", "40247DEE24A970DE1996164FBFF0A8"),
        ("atan(1)", atan(n(1)), "40C90FDAA22168C2", "40C90FDAA22168C234C4C6628B80DC"),
        ("atan(10)", atan(n(10)), "411789BD2C160054", "411789BD2C16005382EABF0CD4B6AB"),
        ("atan(-0.5)", atan(half().neg()), "C076B19C1586ED3E", "C076B19C1586ED3DA2B7F222F65E1D"),
        ("asin(0.5)", asin(half()).unwrap(), "40860A91C16B9B2C", "40860A91C16B9B2C232DD99707AB3D"),
        ("acos(0.5)", acos(half()).unwrap(), "4110C152382D7366", "4110C152382D73658465BB32E0F568"),
        ("acos(-1)", acos(n(-1)).unwrap(), "413243F6A8885A31", "413243F6A8885A308D313198A2E037"),
        ("pi", pi(), "413243F6A8885A31", "413243F6A8885A308D313198A2E037"),
        ("e", e(), "412B7E151628AED3", "412B7E151628AED2A6ABF7158809CF"),
        ("annuity(0.25, 4)", annuity(half().scaled(-1), 4).unwrap(), "406C66AD711581BC", "406C66AD711581BC02C66AD711581C"),
        ("present-value(0.5, 1 2 3)", present_value(half(), &[n(1), n(2), n(3)]).unwrap(), "41271C71C71C71C7", "41271C71C71C71C71C71C71C71C71C"),
    ];
    for (name, value, long, extended) in cases {
        check(name, value, long, extended);
    }
}

#[test]
fn arguments_outside_a_domain_have_no_value() {
    assert_eq!(sqrt(n(-1)), None);
    assert_eq!(ln(Real::ZERO), None);
    assert_eq!(log10(n(-2)), None);
    assert_eq!(asin(n(2)), None);
    assert_eq!(acos(half().add(n(-2))), None);
    assert_eq!(annuity(n(-1), 3), None);
    assert_eq!(annuity(half(), 0), None);
    assert_eq!(present_value(n(-1), &[n(1)]), None);
    assert_eq!(sin(n(1).scaled(80)), None);
}

#[test]
fn exact_values_stay_exact() {
    assert_eq!(sqrt(n(144)).unwrap().to_hfp(Precision::Long).unwrap(), Hfp::from_integer(12, Precision::Long));
    assert_eq!(exp(Real::ZERO), Real::ONE);
    assert_eq!(ln(Real::ONE).unwrap().to_hfp(Precision::Long).unwrap(), Hfp::zero(Precision::Long));
    assert_eq!(annuity(Real::ZERO, 4).unwrap().to_hfp(Precision::Long).unwrap(), hfp("4040000000000000", Precision::Long));
    assert_eq!(asin(n(1)).unwrap(), half_pi());
    assert_eq!(acos(n(1)).unwrap(), Real::ZERO);
}

#[test]
fn a_result_beyond_hfp_is_an_exponent_overflow_and_below_it_zero() {
    assert_eq!(exp(n(200)).to_hfp(Precision::Long), Err(zarch::check::ProgramCheck::HfpExponentOverflow));
    assert_eq!(exp(n(-200)).to_hfp(Precision::Long), Ok(Hfp::zero(Precision::Long)));
}

#[test]
fn statistics_follow_the_language_reference() {
    let values = [n(2), n(4), n(4), n(4), n(5), n(5), n(7), n(9)];
    let long = |r: Real| r.to_hfp(Precision::Long).unwrap();
    assert_eq!(long(mean(&values).unwrap()), Hfp::from_integer(5, Precision::Long));
    assert_eq!(long(variance(&values).unwrap()), Hfp::from_integer(4, Precision::Long));
    assert_eq!(long(variance(&values).unwrap().sqrt()), Hfp::from_integer(2, Precision::Long));
    assert_eq!(long(median(&values).unwrap()), long(n(9).scaled(-1)));
    assert_eq!(long(median(&[n(3), n(1), n(2)]).unwrap()), Hfp::from_integer(2, Precision::Long));
    assert_eq!(long(midrange(&values).unwrap()), long(n(11).scaled(-1)));
    assert_eq!(variance(&[n(7)]).unwrap(), Real::ZERO);
    assert_eq!(mean(&[]), None);
}

#[test]
fn hfp_values_pass_through_exactly() {
    for bits in [0x4110_0000_0000_0000u64, 0xC21A_BCDE_F012_3456, 0x0010_0000_0000_0001, 0x7FFF_FFFF_FFFF_FFFF] {
        let h = Hfp::from_bytes(Precision::Long, &bits.to_be_bytes());
        assert_eq!(Real::from_hfp(h).to_hfp(Precision::Long).unwrap(), h);
    }
}

#[test]
fn rounding_to_an_integer_takes_ties_away_from_zero() {
    assert_eq!(n(5).scaled(-1).round_to_integer(), Some(3));
    assert_eq!(n(-5).scaled(-1).round_to_integer(), Some(-3));
    assert_eq!(n(3).scaled(-3).round_to_integer(), Some(0));
    assert_eq!(half().round_to_integer(), Some(1));
    assert_eq!(Real::ZERO.round_to_integer(), Some(0));
    assert_eq!(n(1).scaled(101).round_to_integer(), None);
}
