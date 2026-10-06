use crate::{Case, hex};
use numeric::assumptions as a;
use numeric::binary::{self, Binary};
use numeric::float;
use numeric::precision::{Fixed, Places};
use numeric::sign;
use numeric::{Arith, Options, Trunc};
use std::cmp::Ordering;
use zarch::check::ProgramMask;
use zarch::decimal::{self, Decimal};
use zarch::ebcdic::{CodePage, Collation, compare_alphanumeric};
use zarch::hfp::{Hfp, Precision};

pub fn all(options: &Options, with_national: bool) -> Vec<Case> {
    let mut cases = Vec::new();
    if with_national {
        cases.extend(national_of());
    }
    cases.extend(collation());
    cases.extend(packed_moves(options));
    cases.extend(packed_compares(options));
    cases.extend(result_signs());
    cases.extend(zoned_digits());
    cases.extend(trunc(options));
    cases.extend(arith(options));
    cases.extend(hfp(options));
    cases
}

fn case(name: String, assumptions: Vec<&'static str>) -> Case {
    Case { name, assumptions, ibm_only: false, operands: vec![], items: vec![], statements: vec![], expect: vec![] }
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(|l| l.trim().trim_start_matches('|').trim_start().to_owned()).filter(|l| !l.is_empty()).collect()
}

fn packed(len: usize, value: Fixed) -> Vec<u8> {
    let mut out = vec![0; len];
    let magnitude = value.magnitude.to_u128().expect("31 digits at most");
    decimal::encode(&mut out, Decimal { negative: value.negative, magnitude }).unwrap();
    out
}

fn packed_int(len: usize, value: i128) -> Vec<u8> {
    packed(len, Fixed::new(value, Places::new(31, 0)))
}

/// 'L', 'E' or 'G' as the invariant EBCDIC byte.
fn ordering_byte(o: Ordering) -> u8 {
    match o {
        Ordering::Less => 0xD3,
        Ordering::Equal => 0xC5,
        Ordering::Greater => 0xC7,
    }
}

fn classify(left: &str, right: &str, result: &str) -> Vec<String> {
    lines(&format!(
        "IF {left} < {right}
        |    MOVE 'L' TO {result}
        |ELSE
        |    IF {left} = {right}
        |        MOVE 'E' TO {result}
        |    ELSE
        |        MOVE 'G' TO {result}
        |    END-IF
        |END-IF"
    ))
}

/// Each code page's conversion to UTF-16 through the compiler's own conversion services. A mixed
/// page reads X'0E' and X'0F' as shifts, which are no characters, so national spaces pad the rest.
fn national_of() -> Vec<Case> {
    let bytes: Vec<u8> = (0..=255).collect();
    CodePage::all()
        .iter()
        .map(|page| Case {
            ibm_only: true,
            items: vec!["05 N-% PIC N(256) USAGE NATIONAL.".into()],
            statements: vec![format!("MOVE FUNCTION NATIONAL-OF(ALL-BYTES, {}) TO N-%", page.ccsid)],
            expect: page.to_utf16be(&bytes).into_iter().chain([0x00, 0x20].into_iter().cycle()).take(512).collect(),
            ..case(format!("nat.{}", page.ccsid), vec![a::CCSID_TABLES])
        })
        .collect()
}

fn collation() -> Vec<Case> {
    let pairs: [(&[u8], &[u8]); 5] =
        [(&[0xC1], &[0x81]), (&[0xE9], &[0xF0]), (&[0xC1], &[0xC1, 0x40, 0x40]), (&[0xC1], &[0xC1, 0x00]), (&[0x40], &[0x00])];
    pairs
        .iter()
        .enumerate()
        .map(|(i, (x, y))| {
            let result = ordering_byte(compare_alphanumeric(x, y, &Collation::Native));
            let mut statements = vec![format!("MOVE X'{}' TO A-%", hex(x)), format!("MOVE X'{}' TO B-%", hex(y))];
            statements.extend(classify("A-%", "B-%", "R-%"));
            Case {
                items: vec![format!("05 A-% PIC X({}).", x.len()), format!("05 B-% PIC X({}).", y.len()), "05 R-% PIC X.".into()],
                statements,
                expect: [*x, *y, &[result]].concat(),
                ..case(format!("col.{i}"), vec![])
            }
        })
        .collect()
}

fn packed_moves(options: &Options) -> Vec<Case> {
    (0xAu8..=0xF)
        .map(|s| {
            let source = [0x12, 0x30 | s];
            Case {
                items: lines("05 S-% PIC S9(3) COMP-3.\n05 SX-% REDEFINES S-% PIC X(2).\n05 R-% PIC S9(3) COMP-3."),
                statements: vec![format!("MOVE X'{}' TO SX-%", hex(&source)), "MOVE S-% TO R-%".into()],
                expect: [&source[..], &sign::move_packed(&source, true, options.numproc)].concat(),
                ..case(format!("pmove.{s:X}"), vec![a::PFD_MOVES_BYTES])
            }
        })
        .collect()
}

fn packed_compares(options: &Options) -> Vec<Case> {
    [(0x1Fu8, 0x1Cu8), (0x0D, 0x0C), (0x1A, 0x1C), (0x1C, 0x2C)]
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let result = ordering_byte(sign::compare_packed(&[x], &[y], options.numproc).expect("valid signs"));
            let mut statements = vec![format!("MOVE X'{x:02X}' TO SX-%"), format!("MOVE X'{y:02X}' TO TX-%")];
            statements.extend(classify("S-%", "T-%", "R-%"));
            Case {
                items: lines(
                    "05 S-% PIC S9 COMP-3.\n05 SX-% REDEFINES S-% PIC X.\n05 T-% PIC S9 COMP-3.\n05 TX-% REDEFINES T-% PIC X.\n05 R-% PIC X.",
                ),
                statements,
                expect: vec![x, y, result],
                ..case(format!("pcmp.{i}"), vec![a::PFD_COMPARES_LOGICALLY])
            }
        })
        .collect()
}

fn result_signs() -> Vec<Case> {
    vec![Case {
        items: lines(
            "05 U-% PIC 9(3) COMP-3.\n05 UX-% REDEFINES U-% PIC X(2).\n05 V-% PIC S9(3) COMP-3.\n05 VX-% REDEFINES V-% PIC X(2).",
        ),
        statements: lines("MOVE X'123C' TO UX-%\nADD 1 TO U-%\nMOVE X'123A' TO VX-%\nADD 1 TO V-%"),
        expect: vec![0x12, 0x40 | sign::preferred(false, false), 0x12, 0x40 | sign::preferred(false, true)],
        ..case("sign.0".into(), vec![a::PREFERRED_RESULT_SIGNS])
    }]
}

/// Zoned operands pass through PACK, which discards every zone but the sign's.
fn zoned_digits() -> Vec<Case> {
    [([0xF1u8, 0x40, 0xF3], 104u128), ([0xF1, 0xF2, 0xC3], 124)]
        .iter()
        .enumerate()
        .map(|(i, &(zoned, sum))| {
            let mut p = [0u8; 2];
            decimal::encode(&mut p, Decimal { negative: false, magnitude: sum }).unwrap();
            let mut z = [0u8; 3];
            decimal::unpk(&mut z, &p).unwrap();
            z[2] = (z[2] & 0x0F) | sign::preferred(false, false) << 4;
            Case {
                items: lines("05 Z-% PIC 9(3).\n05 ZX-% REDEFINES Z-% PIC X(3)."),
                statements: vec![format!("MOVE X'{}' TO ZX-%", hex(&zoned)), "ADD 1 TO Z-%".into()],
                expect: z.to_vec(),
                ..case(format!("zoned.{i}"), vec![a::ZONED_BY_PACK, a::PREFERRED_RESULT_SIGNS])
            }
        })
        .collect()
}

fn trunc(options: &Options) -> Vec<Case> {
    let tags = if options.trunc == Trunc::Opt { vec![a::TRUNC_OPT_IS_BINARY] } else { vec![] };
    [(false, 12345i128, false), (false, 9999, false), (false, 70000, false), (true, 40000, false), (true, -12345, false), (false, 12345, true)]
        .iter()
        .enumerate()
        .map(|(i, &(signed, value, by_move))| {
            let item = Binary { digits: 4, signed, native: numeric::Native::No };
            let stored = binary::store(item, value, options);
            Case {
                items: vec![
                    format!("05 B-% PIC {}9(4) COMP.", if signed { "S" } else { "" }),
                    format!("05 V-% PIC S9(5) COMP-3 VALUE {value}."),
                ],
                statements: vec![if by_move { "MOVE V-% TO B-%".into() } else { "COMPUTE B-% = V-%".into() }],
                expect: [stored.bytes, packed_int(3, value)].concat(),
                ..case(format!("trunc.{i}"), tags.clone())
            }
        })
        .collect()
}

fn arith(options: &Options) -> Vec<Case> {
    let mode = options.arith;
    let nines18 = Fixed::new(999_999_999_999_999_999, Places::new(18, 0));
    let nines10_8 = Fixed::new(999_999_999_999_999_999, Places::new(10, 8));
    let evaluate = |x: Fixed, dmax: u32, receiver: Places| {
        let r = x.mul(x, dmax, mode).and_then(|p| p.div(x, dmax, mode)).expect("within the model");
        r.to_receiver(receiver, false).0
    };
    [("S9(18)", "999999999999999999", nines18, 0, Places::new(18, 0)), ("S9(10)V9(8)", "9999999999.99999999", nines10_8, 8, Places::new(10, 8))]
        .iter()
        .enumerate()
        .map(|(i, &(picture, literal, x, dmax, receiver))| Case {
            operands: ["A", "B", "C"].iter().map(|n| format!("05 {n}-% PIC {picture} COMP-3 VALUE {literal}.")).collect(),
            items: vec![format!("05 R-% PIC {picture} COMP-3.")],
            statements: vec!["COMPUTE R-% = A-% * B-% / C-%".into()],
            expect: packed(10, evaluate(x, dmax, receiver)),
            ..case(format!("arith.{i}"), vec![a::INTERMEDIATE_TABLE, a::PREFERRED_RESULT_SIGNS])
        })
        .collect()
}

fn long(bits: u64) -> Hfp {
    Hfp::from_bytes(Precision::Long, &bits.to_be_bytes())
}

fn hfp(options: &Options) -> Vec<Case> {
    let p = options.arith.float_intermediate();
    let mask = ProgramMask::default();
    let narrowing = if options.arith == Arith::Extend { vec![a::FLOAT_NARROWING_ROUNDS] } else { vec![] };
    let (one, two, three) = (long(0x4110_0000_0000_0000), long(0x4120_0000_0000_0000), long(0x4130_0000_0000_0000));
    let operands = lines("05 A-% COMP-2.\n05 AX-% REDEFINES A-% PIC X(8).\n05 B-% COMP-2.\n05 BX-% REDEFINES B-% PIC X(8).");
    let plant = |x: Hfp, y: Hfp| vec![format!("MOVE X'{}' TO AX-%", hex(&x.to_bytes())), format!("MOVE X'{}' TO BX-%", hex(&y.to_bytes()))];
    let at = |v: Hfp| v.lengthen(p);
    let stored = |v: Hfp, target: Precision| if v.precision.digits() > target.digits() { float::narrow_rounded(v, target).unwrap() } else { v };

    let two_thirds = stored(at(two).div(at(three), mask).unwrap(), Precision::Long);
    let third_times_three = stored(at(one).div(at(three), mask).and_then(|t| t.mul(at(three), p, mask)).unwrap(), Precision::Long);
    let wide = long(0x4112_3456_8000_0000);
    let tenth = float::from_fixed(Fixed::new(1, Places::new(1, 1)), p, mask).map(|v| stored(v, Precision::Long)).unwrap();
    let almost_tenth = long(0x4019_9999_9999_9999);
    let to_hundredths = || packed(2, float::to_receiver(almost_tenth, Places::new(1, 2)).0);

    vec![
        Case {
            operands: operands.clone(),
            items: vec!["05 D-% COMP-2.".into()],
            statements: [plant(two, three), vec!["COMPUTE D-% = A-% / B-%".into()]].concat(),
            expect: two_thirds.to_bytes(),
            ..case("hfp.0".into(), narrowing.clone())
        },
        Case {
            operands: operands.clone(),
            items: vec!["05 D-% COMP-2.".into()],
            statements: [plant(one, three), vec!["COMPUTE D-% = A-% / B-% * B-%".into()]].concat(),
            expect: third_times_three.to_bytes(),
            ..case("hfp.1".into(), narrowing)
        },
        Case {
            operands: operands.clone(),
            items: vec!["05 F-% COMP-1.".into()],
            statements: [plant(wide, one), vec!["MOVE A-% TO F-%".into()]].concat(),
            expect: stored(wide, Precision::Short).to_bytes(),
            ..case("hfp.2".into(), vec![a::FLOAT_NARROWING_ROUNDS])
        },
        Case {
            operands: vec!["05 P-% PIC S9V9 COMP-3 VALUE 0.1.".into()],
            items: vec!["05 D-% COMP-2.".into()],
            statements: vec!["COMPUTE D-% = P-%".into()],
            expect: tenth.to_bytes(),
            ..case("hfp.3".into(), vec![a::FLOAT_FROM_DECIMAL])
        },
        Case {
            operands: operands.clone(),
            items: vec!["05 Q-% PIC S9V99 COMP-3.".into()],
            statements: [plant(almost_tenth, one), vec!["COMPUTE Q-% = A-%".into()]].concat(),
            expect: to_hundredths(),
            ..case("hfp.4".into(), vec![a::FLOAT_TO_DECIMAL])
        },
        Case {
            operands,
            items: vec!["05 Q-% PIC S9V99 COMP-3.".into()],
            statements: [plant(almost_tenth, one), vec!["COMPUTE Q-% ROUNDED = A-%".into()]].concat(),
            expect: to_hundredths(),
            ..case("hfp.5".into(), vec![a::FLOAT_TO_DECIMAL])
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(tokens: &[&str]) -> Options {
        let mut o = Options::default();
        for t in tokens {
            o.apply(t).unwrap();
        }
        o
    }

    fn expect(cases: &[Case], name: &str) -> String {
        hex(&cases.iter().find(|c| c.name == name).unwrap().expect)
    }

    #[test]
    fn predictions_differ_where_the_options_do() {
        let (std, opt, bin) = (all(&with(&["TRUNC(STD)"]), false), all(&with(&["TRUNC(OPT)"]), false), all(&with(&["TRUNC(BIN)"]), false));
        assert_eq!(expect(&std, "trunc.0"), "092912345C");
        assert_eq!(expect(&bin, "trunc.0"), "303912345C");
        assert_eq!(expect(&opt, "trunc.0"), expect(&bin, "trunc.0"));
        let (nopfd, pfd) = (all(&with(&["NUMPROC(NOPFD)"]), false), all(&with(&["NUMPROC(PFD)"]), false));
        assert_eq!(expect(&nopfd, "pcmp.0"), "1F1CC5");
        assert_eq!(expect(&pfd, "pcmp.0"), "1F1CC7");
        assert_eq!(expect(&nopfd, "pmove.A"), "123A123C");
        assert_eq!(expect(&pfd, "pmove.A"), "123A123A");
    }

    #[test]
    fn extend_keeps_one_more_digit_of_an_18_by_18_digit_product() {
        let (compat, extend) = (all(&with(&["ARITH(COMPAT)"]), false), all(&with(&["ARITH(EXTEND)"]), false));
        assert_ne!(expect(&compat, "arith.0"), expect(&extend, "arith.0"));
    }

    #[test]
    fn hfp_predictions() {
        let compat = all(&Options::default(), false);
        assert_eq!(expect(&compat, "hfp.0"), "40AAAAAAAAAAAAAA");
        assert_eq!(expect(&compat, "hfp.1"), "40FFFFFFFFFFFFFF");
        assert_eq!(expect(&compat, "hfp.2"), "41123457");
        assert_eq!(expect(&compat, "hfp.3"), "4019999999999999");
        assert_eq!(expect(&compat, "hfp.4"), "010C");
        assert_eq!(expect(&compat, "hfp.5"), "010C");
    }

    #[test]
    fn an_embedded_space_counts_as_zero() {
        assert_eq!(expect(&all(&Options::default(), false), "zoned.0"), "F1F0F4");
    }
}
