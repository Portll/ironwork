use super::*;

fn page() -> &'static CodePage {
    CodePage::by_ccsid(1140).unwrap()
}

#[test]
fn lilian_days_start_on_15_october_1582() {
    assert_eq!(lilian(1582, 10, 15), 1);
    assert_eq!(lilian(1988, 5, 16), 148_138);
    assert_eq!(lilian(1990, 6, 4), 148_887);
    assert_eq!(lilian(2000, 1, 1), 152_385);
    assert_eq!(lilian(2024, 2, 29), 161_210);
    assert_eq!(lilian(1970, 1, 1), 141_428);
    assert_eq!(LAST_LILIAN, 3_074_324);
    assert_eq!(Stamp::from_unix(0, 0), Stamp { lilian: 141_428, millis: 0 });
    assert_eq!((weekday(1), weekday(148_138), weekday(152_385)), (6, 2, 7));
    let f = Stamp { lilian: 152_444, millis: 0 }.fields();
    assert_eq!((f.year, f.month, f.day, f.yday), (2000, 2, 29, 60));
}

#[test]
fn condition_tokens_carry_severity_number_and_facility() {
    assert_eq!(INSUFFICIENT.symbol(), "CEE2EB");
    assert_eq!(INSUFFICIENT.token(), [0x00, 0x03, 0x09, 0xCB, 0x59, 0xC3, 0xC5, 0xC5, 0, 0, 0, 0]);
    assert_eq!(DATE_VALUE.token()[..8], [0x00, 0x03, 0x09, 0xCC, 0x59, 0xC3, 0xC5, 0xC5]);
    assert_eq!((DATE_TRUNCATED.symbol(), DATE_TRUNCATED.token()[4]), ("CEE2EU".to_owned(), 0x51));
    assert_eq!((DUMP_OPTIONS.symbol(), DESTINATION.symbol(), FREE_ADDRESS.symbol()), ("CEE30U".to_owned(), "CEE0E3".to_owned(), "CEE0PA".to_owned()));
}

#[test]
fn lilian_seconds_are_long_hfp_to_the_millisecond() {
    for ms in [86_400_000, 12_799_191_601_078, 12_799_191_661_986, 265_621_679_999_999] {
        let bytes = seconds_hfp(ms);
        assert_eq!(bytes[0] & 0x80, 0);
        assert_eq!(hfp_millis(bytes), Some(ms));
    }
    assert_eq!(seconds_hfp(86_400_000), [0x45, 0x15, 0x18, 0, 0, 0, 0, 0]);
    assert_eq!(seconds_hfp(0), [0; 8]);
}

#[test]
fn picture_terms_and_delimiters() {
    let t = |s: &str| terms(&page().encode(s).unwrap(), page());
    assert_eq!(t("YYYY-MM-DD"), [Term::Year(4), Term::Literal(0x60), Term::Month(false), Term::Literal(0x60), Term::Day(false)]);
    assert_eq!(t("HH:MM"), [Term::Hour(false), Term::Literal(0x7A), Term::Minute]);
    assert_eq!(t("Mmmmmmmmmz ZD")[0], Term::MonthName { upper: [true, false, false, false, false, false, false, false, false].to_vec(), trim: true });
    assert_eq!(t("MMMMMMMMMZD")[1], Term::Day(true));
    assert_eq!(t("<JJJJ> YY.MM.DD")[0], Term::Era);
    assert_eq!(t("S P"), [Term::Literal(0xE2), Term::Literal(0x40), Term::Literal(0xD7)]);
}

#[test]
fn dump_options_and_the_s806_message() {
    assert_eq!(dump_options("TRACE FILE VAR STOR"), ("CEEDUMP".to_owned(), false));
    assert_eq!(dump_options("NOTRACEBACK,FNAME(MYDUMP) BLOCKS"), ("MYDUMP".to_owned(), false));
    assert!(dump_options("TRACE BOGUS").1);
    assert!(missing("CEEHDLR").contains("Language Environment callable service"));
    assert!(missing("CEESDLOG").contains("Language Environment callable service"));
    assert!(missing("CEEXYZ").contains("no such program"));
}

#[test]
fn ceeigzct_holds_every_condition_the_services_return() {
    let named: Vec<(u16, u8)> = crate::feedback::conditions().collect();
    let returned = [
        DESTINATION, HEAP_ID, HEAP_SIZE, FREE_ADDRESS, HEAP_SHORT, SECONDS_RANGE, INSUFFICIENT, DATE_VALUE, HOURS, LILIAN_RANGE, DATE_RANGE,
        MINUTES, MONTH, PICTURE, SECONDS_VALUE, DAYS_NONNUMERIC, SECS_NONNUMERIC, DATE_TRUNCATED, TIMESTAMP_TRUNCATED, DUMP_OPTIONS,
    ];
    for c in returned {
        assert!(named.contains(&(c.number, c.severity)), "{c:?}");
        assert!(!c.text().is_empty(), "{c:?}");
    }
}
