//! The SQL descriptor area as a COBOL program declares it (Db2 13 for z/OS SQL, SQL descriptor
//! area): a 16-byte header of SQLDAID, SQLDABC, SQLN and SQLD, then 44-byte SQLVARs of SQLTYPE,
//! SQLLEN, SQLDATA, SQLIND and SQLNAME, read and written by offset from the descriptor's start.

use super::{Column, ColumnType, HostType, SqlError, Value, write};
use crate::lir::SqlNames;
use crate::unit::ADDRESS_BASE;
use crate::vocab::{SignClause, SignPosition};
use zarch::ebcdic::CodePage;

const HEADER: usize = 16;
const SQLVAR: usize = 44;
/// The scale DESCRIBE gives a decimal the backend gives no precision or scale, as DECIMAL(31,s)
/// (assumption C403).
const NUMERIC_SCALE: u8 = 6;

/// SQLCODE -804: an SQLDA the statement cannot use, with Db2's reason code as its message token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Invalid(pub u8);

pub(super) const INVALID: SqlError = SqlError { code: -804, state: "07002" };

fn halfword(mem: &[u8], at: usize) -> i16 {
    i16::from_be_bytes([mem[at], mem[at + 1]])
}

fn fullword(mem: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([mem[at], mem[at + 1], mem[at + 2], mem[at + 3]])
}

/// SQLTYPE, without its null bit, and SQLLEN for a column (SQL Reference, SQLTYPE and SQLLEN
/// fields); None for a type with no Db2 counterpart.
fn sqltype(ty: &ColumnType) -> Option<(i16, [u8; 2])> {
    let length = |n: u16| n.to_be_bytes();
    Some(match *ty {
        ColumnType::Char(n) => (452, length(n)),
        ColumnType::VarChar(n) => (448, length(n)),
        ColumnType::Graphic(n) => (468, length(n)),
        ColumnType::VarGraphic(n) => (464, length(n)),
        ColumnType::SmallInt => (500, length(2)),
        ColumnType::Integer => (496, length(4)),
        ColumnType::BigInt => (492, length(8)),
        ColumnType::Decimal { precision, scale } => (484, [precision, scale]),
        ColumnType::Numeric => (484, [31, NUMERIC_SCALE]),
        ColumnType::Real => (480, length(4)),
        ColumnType::Double => (480, length(8)),
        ColumnType::Date => (384, length(10)),
        ColumnType::Time => (388, length(8)),
        ColumnType::Timestamp(0) => (392, length(19)),
        ColumnType::Timestamp(p) => (392, length(20 + u16::from(p))),
        ColumnType::Binary(n) => (912, length(n)),
        ColumnType::VarBinary(n) => (908, length(n)),
        ColumnType::Other(_) => return None,
    })
}

/// DESCRIBE OUTPUT into the SQLDA at `at`: SQLDAID, SQLDABC and SQLD always, and an SQLVAR for each
/// column when SQLN gives room for them all; the bytes written. `columns` is None for a statement
/// that is not a query. No backend keeps column labels, so LABELS gives each SQLNAME length 0 and
/// ANY the name. Err(Ok) is -804; Err(Err(why)) a column whose type Db2 has no counterpart for.
pub(super) fn describe(mem: &mut [u8], at: usize, columns: Option<&[Column]>, names: SqlNames, page: &'static CodePage) -> Result<usize, Result<Invalid, String>> {
    if at + HEADER > mem.len() {
        return Err(Ok(Invalid(7)));
    }
    let sqln = halfword(mem, at + 12);
    let columns = columns.unwrap_or_default();
    let sqld = i16::try_from(columns.len()).map_err(|_| Ok(Invalid(7)))?;
    let described = sqld > 0 && sqld <= sqln;
    if sqln < 0 || at + HEADER + if described { SQLVAR * columns.len() } else { 0 } > mem.len() {
        return Err(Ok(Invalid(7)));
    }
    let put = |mem: &mut [u8], from: usize, len: usize, value: Value, ty: HostType| {
        let _ = write(&value, &mut mem[from..from + len], &ty, page);
    };
    put(mem, at, 8, Value::Char("SQLDA".into()), HostType::Char(8));
    put(mem, at + 8, 4, Value::Int(i64::from(sqln) * SQLVAR as i64 + HEADER as i64), HostType::Integer { signed: true });
    put(mem, at + 14, 2, Value::Int(sqld.into()), HostType::SmallInt { signed: true });
    if !described {
        return Ok(HEADER);
    }
    for (k, column) in columns.iter().enumerate() {
        let Some((code, length)) = sqltype(&column.ty) else {
            let ColumnType::Other(name) = &column.ty else { unreachable!("sqltype describes every other type") };
            return Err(Err(format!("column {} is {name}, which has no Db2 type", column.name)));
        };
        let var = at + HEADER + SQLVAR * k;
        mem[var..var + 2].copy_from_slice(&(code + i16::from(column.nullable)).to_be_bytes());
        mem[var + 2..var + 4].copy_from_slice(&length);
        let ccsid = match column.ty {
            ColumnType::Char(_) | ColumnType::VarChar(_) | ColumnType::Date | ColumnType::Time | ColumnType::Timestamp(_) => Some(page.ccsid),
            ColumnType::Graphic(_) | ColumnType::VarGraphic(_) => page.dbcs_ccsid(),
            _ => None,
        };
        mem[var + 4..var + 8].copy_from_slice(&u32::from(ccsid.unwrap_or(0)).to_be_bytes());
        mem[var + 8..var + 12].fill(0);
        let name = match names {
            SqlNames::Labels => Vec::new(),
            SqlNames::Names | SqlNames::Any => sqlname(&column.name, page),
        };
        mem[var + 12..var + 14].copy_from_slice(&(name.len() as i16).to_be_bytes());
        mem[var + 14..var + 14 + name.len()].copy_from_slice(&name);
        mem[var + 14 + name.len()..var + SQLVAR].fill(page.encode_char(' ').unwrap_or(0x40));
    }
    Ok(HEADER + SQLVAR * columns.len())
}

/// A column name as SQLNAME holds it: in the code page, a character it lacks as its `?`, and as
/// many whole characters as fit 30 bytes.
fn sqlname(name: &str, page: &CodePage) -> Vec<u8> {
    let mut chars = name.chars().count().min(30);
    loop {
        let bytes = page.encode_lossy(&name.chars().take(chars).collect::<String>());
        if bytes.len() <= 30 {
            return bytes;
        }
        chars -= 1;
    }
}

/// One SQLVAR of a USING DESCRIPTOR: the host variable SQLDATA addresses, its type from SQLTYPE and
/// SQLLEN, and the indicator SQLIND addresses when SQLTYPE is odd.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Var {
    pub offset: usize,
    pub len: usize,
    pub ty: HostType,
    pub indicator: Option<usize>,
}

/// SQLTYPE and SQLLEN as a host variable's type and the bytes it takes.
fn host_type(code: i16, sqllen: [u8; 2]) -> Option<(HostType, usize)> {
    let n = usize::from(u16::from_be_bytes(sqllen));
    let (precision, scale) = (u32::from(sqllen[0]), u32::from(sqllen[1]));
    let n32 = n as u32;
    Some(match code {
        452 | 384 | 388 | 392 => (HostType::Char(n32), n),
        448 | 456 => (HostType::VarChar(n32), 2 + n),
        468 => (HostType::Graphic(n32), 2 * n),
        464 | 472 => (HostType::VarGraphic(n32), 2 + 2 * n),
        500 => (HostType::SmallInt { signed: true }, 2),
        496 => (HostType::Integer { signed: true }, 4),
        492 => (HostType::BigInt { signed: true }, 8),
        480 if n == 4 => (HostType::Real, 4),
        480 if n == 8 => (HostType::Double, 8),
        484 if (1..=31).contains(&precision) && scale <= precision => (HostType::Decimal { digits: precision, scale, signed: true }, precision as usize / 2 + 1),
        504 if (1..=31).contains(&precision) && scale <= precision => {
            let sign = Some(SignClause { position: SignPosition::Leading, separate: true });
            (HostType::Zoned { digits: precision, scale, signed: true, sign }, precision as usize + 1)
        }
        _ => return None,
    })
}

/// An address in the SQLDA as an offset in run-unit memory holding `len` bytes there.
fn addressed(address: u32, len: usize, mem_len: usize) -> Option<usize> {
    let offset = address.checked_sub(ADDRESS_BASE)? as usize;
    (address != 0 && offset + len <= mem_len).then_some(offset)
}

/// The host variables the SQLDA at `at` describes, SQLD of them, for input (EXECUTE, OPEN) or
/// output (FETCH): Db2's -804 reasons where it cannot be used.
pub(super) fn vars(mem: &[u8], at: usize, input: bool) -> Result<Vec<Var>, Invalid> {
    if at + HEADER > mem.len() {
        return Err(Invalid(7));
    }
    let (sqldabc, sqln, sqld) = (fullword(mem, at + 8) as i32, halfword(mem, at + 12), halfword(mem, at + 14));
    if sqln < 0 || sqld < 0 {
        return Err(Invalid(7));
    }
    if i64::from(sqldabc) < i64::from(sqln) * SQLVAR as i64 + HEADER as i64 {
        return Err(Invalid(14));
    }
    if sqld > sqln {
        return Err(Invalid(11));
    }
    if at + HEADER + SQLVAR * sqld as usize > mem.len() {
        return Err(Invalid(7));
    }
    let (unknown, bad_pointer) = if input { (8, 12) } else { (16, 13) };
    (0..sqld as usize)
        .map(|k| {
            let var = at + HEADER + SQLVAR * k;
            let code = halfword(mem, var);
            let (ty, len) = host_type(code & !1, [mem[var + 2], mem[var + 3]]).ok_or(Invalid(unknown))?;
            let offset = addressed(fullword(mem, var + 4), len, mem.len()).ok_or(Invalid(bad_pointer))?;
            let indicator = match code & 1 {
                0 => None,
                _ => Some(addressed(fullword(mem, var + 8), 2, mem.len()).ok_or(Invalid(bad_pointer))?),
            };
            Ok(Var { offset, len, ty, indicator })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> &'static CodePage {
        CodePage::by_ccsid(1140).expect("1140")
    }

    fn sqlda(sqln: i16) -> Vec<u8> {
        let mut mem = vec![0u8; 16 + 44 * sqln.max(0) as usize + 64];
        mem[12..14].copy_from_slice(&sqln.to_be_bytes());
        mem
    }

    #[test]
    fn describe_fills_the_header_and_each_sqlvar() {
        let columns = [
            Column { name: "NAME".into(), ty: ColumnType::Char(10), nullable: false },
            Column { name: "AMT".into(), ty: ColumnType::Decimal { precision: 7, scale: 2 }, nullable: true },
        ];
        let mut mem = sqlda(3);
        describe(&mut mem, 0, Some(&columns), SqlNames::Names, page()).unwrap();
        assert_eq!(&mem[0..8], &[0xE2, 0xD8, 0xD3, 0xC4, 0xC1, 0x40, 0x40, 0x40]);
        assert_eq!((fullword(&mem, 8), halfword(&mem, 12), halfword(&mem, 14)), (16 + 44 * 3, 3, 2));
        assert_eq!((halfword(&mem, 16), &mem[18..20], fullword(&mem, 20)), (452, &[0, 10][..], 1140));
        assert_eq!((halfword(&mem, 28), &mem[30..34]), (4, &[0xD5, 0xC1, 0xD4, 0xC5][..]));
        assert_eq!((halfword(&mem, 60), &mem[62..64], fullword(&mem, 64)), (485, &[7, 2][..], 0));
    }

    #[test]
    fn too_few_sqlvars_or_no_query_set_sqld_alone() {
        let columns = [Column { name: "A".into(), ty: ColumnType::Integer, nullable: false }];
        let mut mem = sqlda(0);
        describe(&mut mem, 0, Some(&columns), SqlNames::Names, page()).unwrap();
        assert_eq!((halfword(&mem, 14), fullword(&mem, 8)), (1, 16));
        let mut mem = sqlda(2);
        describe(&mut mem, 0, None, SqlNames::Names, page()).unwrap();
        assert_eq!((halfword(&mem, 14), halfword(&mem, 16)), (0, 0));
        let other = [Column { name: "B".into(), ty: ColumnType::Other("PostgreSQL type OID 16".into()), nullable: true }];
        assert!(matches!(describe(&mut sqlda(1), 0, Some(&other), SqlNames::Names, page()), Err(Err(why)) if why.contains("OID 16")));
    }

    #[test]
    fn an_unsized_decimal_an_odd_name_and_a_large_sqln() {
        let columns = [Column { name: "TOTAL€✓".into(), ty: ColumnType::Numeric, nullable: true }];
        let mut mem = sqlda(1);
        mem[12..14].copy_from_slice(&1000i16.to_be_bytes());
        describe(&mut mem, 0, Some(&columns), SqlNames::Names, page()).unwrap();
        assert_eq!((halfword(&mem, 16), &mem[18..20]), (485, &[31, 6][..]));
        assert_eq!(halfword(&mem, 28), 7);
        assert_eq!(page().decode(&mem[30..37]), "TOTAL€?");
        assert_eq!(page().decode(&mem[37..60]), " ".repeat(23));
    }

    #[test]
    fn using_descriptor_reads_each_sqlvar_s_type_and_addresses() {
        let mut mem = sqlda(2);
        mem[8..12].copy_from_slice(&(16 + 88u32).to_be_bytes());
        mem[14..16].copy_from_slice(&2i16.to_be_bytes());
        mem[16..18].copy_from_slice(&453i16.to_be_bytes());
        mem[18..20].copy_from_slice(&8u16.to_be_bytes());
        mem[20..24].copy_from_slice(&(ADDRESS_BASE + 110).to_be_bytes());
        mem[24..28].copy_from_slice(&(ADDRESS_BASE + 118).to_be_bytes());
        mem[60..62].copy_from_slice(&484i16.to_be_bytes());
        mem[62..64].copy_from_slice(&[5, 2]);
        mem[64..68].copy_from_slice(&(ADDRESS_BASE + 120).to_be_bytes());
        let got = vars(&mem, 0, true).unwrap();
        assert_eq!(got[0], Var { offset: 110, len: 8, ty: HostType::Char(8), indicator: Some(118) });
        assert_eq!(got[1], Var { offset: 120, len: 3, ty: HostType::Decimal { digits: 5, scale: 2, signed: true }, indicator: None });
        mem[64..68].fill(0);
        assert_eq!(vars(&mem, 0, true), Err(Invalid(12)));
        assert_eq!(vars(&mem, 0, false), Err(Invalid(13)));
        mem[60..62].copy_from_slice(&999i16.to_be_bytes());
        assert_eq!(vars(&mem, 0, true), Err(Invalid(8)));
        mem[14..16].copy_from_slice(&3i16.to_be_bytes());
        assert_eq!(vars(&mem, 0, true), Err(Invalid(11)));
        mem[8..12].copy_from_slice(&16u32.to_be_bytes());
        assert_eq!(vars(&mem, 0, true), Err(Invalid(14)));
    }
}
