//! Hercules as a second reading of the Principles of Operation: a bare-metal program runs one
//! instruction per case and records the result, the condition code and any program interruption;
//! each record is compared with what `zarch` computes. Agreement is evidence, not proof: Hercules
//! is itself an implementation of the same manual.

use crate::unhex;
use std::fs::{self, File};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use zarch::check::{Cc, ProgramCheck, ProgramMask};
use zarch::decimal::{self, Decimal};
use zarch::hfp::{Hfp, Precision};

pub struct Case {
    pub name: String,
    pub op: Op,
}

impl Case {
    pub fn instruction(&self) -> String {
        let name = match &self.op {
            Op::Pack { .. } => "PACK",
            Op::Unpk { .. } => "UNPK",
            Op::Zap { .. } => "ZAP",
            Op::Ap { .. } => "AP",
            Op::Sp { .. } => "SP",
            Op::Mp { .. } => "MP",
            Op::Dp { .. } => "DP",
            Op::Cp { .. } => "CP",
            Op::Srp { .. } => "SRP",
            Op::Tp { .. } => "TP",
            Op::Cvb { .. } => "CVB",
            Op::Cvd { .. } => "CVD",
            Op::Hfp { inst, .. } => return format!("{inst:?}").to_uppercase(),
        };
        name.into()
    }
}

pub enum Op {
    /// SS-format decimal instructions on storage operands (packed or zoned bytes as written).
    Pack { op1_len: usize, op2: Vec<u8> },
    Unpk { op1_len: usize, op2: Vec<u8> },
    Zap { op1: Vec<u8>, op2: Vec<u8> },
    Ap { op1: Vec<u8>, op2: Vec<u8> },
    Sp { op1: Vec<u8>, op2: Vec<u8> },
    Mp { op1: Vec<u8>, op2: Vec<u8> },
    Dp { op1: Vec<u8>, op2: Vec<u8> },
    Cp { op1: Vec<u8>, op2: Vec<u8> },
    /// `shift` is the 6-bit signed amount held in the low bits of D2; `rounding` is 0 to 9.
    Srp { op1: Vec<u8>, shift: u8, rounding: u8 },
    Tp { op: Vec<u8> },
    Cvb { op2: [u8; 8] },
    Cvd { value: i32 },
    /// HFP register-to-register instructions on raw register images. `mask` is the PSW program
    /// mask: 8 fixed-point overflow, 4 decimal overflow, 2 exponent underflow, 1 significance.
    Hfp { inst: HfpInst, op1: Vec<u8>, op2: Vec<u8>, mask: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HfpInst {
    Aer,
    Adr,
    Axr,
    Ser,
    Sdr,
    Sxr,
    Aur,
    Awr,
    /// MULTIPLY (short * short to long), opcode 3C.
    Mer,
    Mdr,
    Mxr,
    Mxdr,
    Der,
    Ddr,
    Cer,
    Cdr,
    Her,
    Hdr,
    Ledr,
    Ldxr,
}

const ALL_HFP: [HfpInst; 20] = [
    HfpInst::Aer,
    HfpInst::Adr,
    HfpInst::Axr,
    HfpInst::Ser,
    HfpInst::Sdr,
    HfpInst::Sxr,
    HfpInst::Aur,
    HfpInst::Awr,
    HfpInst::Mer,
    HfpInst::Mdr,
    HfpInst::Mxr,
    HfpInst::Mxdr,
    HfpInst::Der,
    HfpInst::Ddr,
    HfpInst::Cer,
    HfpInst::Cdr,
    HfpInst::Her,
    HfpInst::Hdr,
    HfpInst::Ledr,
    HfpInst::Ldxr,
];

/// What one case produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub result: Vec<u8>,
    pub cc: Option<u8>,
    pub interruption: Option<u16>,
}

/// Whether the model's outcome is what Hercules produced. An empty model result is a result the
/// model does not define: an HFP instruction that completes and then interrupts.
pub fn agrees(hercules: &Outcome, model: &Outcome) -> bool {
    hercules.cc == model.cc && hercules.interruption == model.interruption && (model.result.is_empty() || hercules.result == model.result)
}

const RESTART_PSW: usize = 0x1A0;
const PROGRAM_NEW_PSW: usize = 0x1D0;
const PROGRAM_OLD_PSW: u16 = 0x150;
const HANDLER: usize = 0x1F00;
const CONSTANTS: usize = 0x1E00;
const WAIT_PSW: usize = CONSTANTS;
const DONE_AT: usize = CONSTANTS + 0x10;
const CR0_SAVE: usize = CONSTANTS + 0x20;
const CODE: usize = 0x2000;
const AREAS: usize = 0x10000;
const AREA: usize = 256;
const DONE: [u8; 4] = [0xC4, 0xD6, 0xD5, 0xC5];
const R1: u8 = 1;
const R2: u8 = 2;
const R3: u8 = 3;
const R4: u8 = 4;

const OFF_OP2: u16 = 32;
const OFF_CC: u16 = 64;
const OFF_INTERRUPTION: u16 = 68;
const OFF_DXC: u16 = 72;
const OFF_MASK: u16 = 76;
const OFF_RESULT: u16 = 80;
const OFF_DONE: u16 = 120;

fn rx(op: u8, r: u8, x: u8, b: u8, d: u16) -> [u8; 4] {
    [op, r << 4 | x, b << 4 | (d >> 8) as u8, d as u8]
}

fn rr(op: u8, r1: u8, r2: u8) -> [u8; 2] {
    [op, r1 << 4 | r2]
}

/// SS with two length or immediate fields, already encoded.
fn ss(op: u8, l1: u8, l2: u8, b1: u8, d1: u16, b2: u8, d2: u16) -> [u8; 6] {
    [op, l1 << 4 | l2, b1 << 4 | (d1 >> 8) as u8, d1 as u8, b2 << 4 | (d2 >> 8) as u8, d2 as u8]
}

fn s_format(op: u16, b: u8, d: u16) -> [u8; 4] {
    [(op >> 8) as u8, op as u8, b << 4 | (d >> 8) as u8, d as u8]
}

fn larl(r: u8, halfwords: i32) -> Vec<u8> {
    let mut out = vec![0xC0, r << 4];
    out.extend(halfwords.to_be_bytes());
    out
}

fn tp_inst(len: usize, b1: u8, d1: u16) -> [u8; 6] {
    [0xEB, (len as u8 - 1) << 4, b1 << 4 | (d1 >> 8) as u8, d1 as u8, 0x00, 0xC0]
}

fn spm(r: u8) -> [u8; 2] {
    [0x04, r << 4]
}

fn ipm(r: u8) -> [u8; 4] {
    [0xB2, 0x22, 0x00, r << 4]
}

fn lpswe(b: u8, d: u16) -> [u8; 4] {
    s_format(0xB2B2, b, d)
}

fn mvc(len: usize, b1: u8, d1: u16, b2: u8, d2: u16) -> [u8; 6] {
    [0xD2, len as u8 - 1, b1 << 4 | (d1 >> 8) as u8, d1 as u8, b2 << 4 | (d2 >> 8) as u8, d2 as u8]
}

fn load_reg(f: u8, d: u16, bytes: usize) -> Vec<u8> {
    match bytes {
        4 => rx(0x78, f, 0, R1, d).to_vec(),
        8 => rx(0x68, f, 0, R1, d).to_vec(),
        _ => [rx(0x68, f, 0, R1, d), rx(0x68, f + 2, 0, R1, d + 8)].concat(),
    }
}

fn store_reg(f: u8, d: u16, bytes: usize) -> Vec<u8> {
    match bytes {
        4 => rx(0x70, f, 0, R1, d).to_vec(),
        8 => rx(0x60, f, 0, R1, d).to_vec(),
        _ => [rx(0x60, f, 0, R1, d), rx(0x60, f + 2, 0, R1, d + 8)].concat(),
    }
}

struct Shape {
    p1: Precision,
    p2: Precision,
    result: Option<Precision>,
    sets_cc: bool,
    opcode: u8,
}

fn shape(inst: HfpInst) -> Shape {
    use HfpInst::*;
    use Precision::{Extended as E, Long as L, Short as S};
    let (p1, p2, result, sets_cc, opcode) = match inst {
        Aer => (S, S, Some(S), true, 0x3A),
        Adr => (L, L, Some(L), true, 0x2A),
        Axr => (E, E, Some(E), true, 0x36),
        Ser => (S, S, Some(S), true, 0x3B),
        Sdr => (L, L, Some(L), true, 0x2B),
        Sxr => (E, E, Some(E), true, 0x37),
        Aur => (S, S, Some(S), true, 0x3E),
        Awr => (L, L, Some(L), true, 0x2E),
        Mer => (S, S, Some(L), false, 0x3C),
        Mdr => (L, L, Some(L), false, 0x2C),
        Mxr => (E, E, Some(E), false, 0x26),
        Mxdr => (L, L, Some(E), false, 0x27),
        Der => (S, S, Some(S), false, 0x3D),
        Ddr => (L, L, Some(L), false, 0x2D),
        Cer => (S, S, None, true, 0x39),
        Cdr => (L, L, None, true, 0x29),
        Her => (S, S, Some(S), false, 0x34),
        Hdr => (L, L, Some(L), false, 0x24),
        Ledr => (L, L, Some(S), false, 0x35),
        Ldxr => (L, E, Some(L), false, 0x25),
    };
    Shape { p1, p2, result, sets_cc, opcode }
}

fn decimal_opcode(op: &Op) -> Option<u8> {
    Some(match op {
        Op::Pack { .. } => 0xF2,
        Op::Unpk { .. } => 0xF3,
        Op::Zap { .. } => 0xF8,
        Op::Cp { .. } => 0xF9,
        Op::Ap { .. } => 0xFA,
        Op::Sp { .. } => 0xFB,
        Op::Mp { .. } => 0xFC,
        Op::Dp { .. } => 0xFD,
        _ => return None,
    })
}

/// The two operand images stored in the case area, and the program mask.
fn operands(op: &Op) -> (Vec<u8>, Vec<u8>, u8) {
    match op {
        Op::Pack { op1_len, op2 } | Op::Unpk { op1_len, op2 } => (vec![0; *op1_len], op2.clone(), 0),
        Op::Zap { op1, op2 } | Op::Ap { op1, op2 } | Op::Sp { op1, op2 } | Op::Mp { op1, op2 } | Op::Dp { op1, op2 } | Op::Cp { op1, op2 } => {
            (op1.clone(), op2.clone(), 0)
        }
        Op::Srp { op1, .. } => (op1.clone(), Vec::new(), 0),
        Op::Tp { op } => (op.clone(), Vec::new(), 0),
        Op::Cvb { op2 } => (Vec::new(), op2.to_vec(), 0),
        Op::Cvd { value } => (Vec::new(), value.to_be_bytes().to_vec(), 0),
        Op::Hfp { op1, op2, mask, .. } => (op1.clone(), op2.clone(), *mask),
    }
}

fn result_at(op: &Op) -> (u16, usize) {
    match op {
        Op::Pack { op1_len, .. } | Op::Unpk { op1_len, .. } => (0, *op1_len),
        Op::Zap { op1, .. } | Op::Ap { op1, .. } | Op::Sp { op1, .. } | Op::Mp { op1, .. } | Op::Dp { op1, .. } | Op::Srp { op1, .. } => {
            (0, op1.len())
        }
        Op::Cp { .. } | Op::Tp { .. } => (0, 0),
        Op::Cvb { .. } => (OFF_RESULT, 4),
        Op::Cvd { .. } => (OFF_RESULT, 8),
        Op::Hfp { inst, .. } => (OFF_RESULT, shape(*inst).result.map_or(0, Precision::bytes)),
    }
}

fn sets_cc(op: &Op) -> bool {
    match op {
        Op::Zap { .. } | Op::Ap { .. } | Op::Sp { .. } | Op::Cp { .. } | Op::Srp { .. } | Op::Tp { .. } => true,
        Op::Hfp { inst, .. } => shape(*inst).sets_cc,
        _ => false,
    }
}

/// The straight-line code for one case, placed at `at`, working on the area at `area`.
fn case_code(op: &Op, at: usize, area: usize) -> Vec<u8> {
    let mut code = larl(R1, ((area as i64 - at as i64) / 2) as i32);
    code.extend(rx(0x58, R2, 0, R1, OFF_MASK));
    code.extend(spm(R2));
    let len = |v: &[u8]| {
        assert!((1..=16).contains(&v.len()), "decimal operands are 1 to 16 bytes");
        v.len() as u8 - 1
    };
    if let Some(opcode) = decimal_opcode(op) {
        let (l1, l2) = match op {
            Op::Pack { op1_len, op2 } | Op::Unpk { op1_len, op2 } => (len(&vec![0; *op1_len]), len(op2)),
            Op::Zap { op1, op2 } | Op::Ap { op1, op2 } | Op::Sp { op1, op2 } | Op::Mp { op1, op2 } | Op::Dp { op1, op2 } | Op::Cp { op1, op2 } => {
                (len(op1), len(op2))
            }
            _ => unreachable!(),
        };
        code.extend(ss(opcode, l1, l2, R1, 0, R1, OFF_OP2));
    }
    match op {
        Op::Srp { op1, shift, rounding } => code.extend(ss(0xF0, len(op1), *rounding, R1, 0, 0, u16::from(*shift & 0x3F))),
        Op::Tp { op } => {
            len(op);
            code.extend(tp_inst(op.len(), R1, 0));
        }
        Op::Cvb { .. } => {
            code.extend(rx(0x4F, R2, 0, R1, OFF_OP2));
            code.extend(rx(0x50, R2, 0, R1, OFF_RESULT));
        }
        Op::Cvd { .. } => {
            code.extend(rx(0x58, R2, 0, R1, OFF_OP2));
            code.extend(rx(0x4E, R2, 0, R1, OFF_RESULT));
        }
        Op::Hfp { inst, .. } => {
            let s = shape(*inst);
            code.extend(load_reg(0, 0, s.p1.bytes()));
            code.extend(load_reg(4, OFF_OP2, s.p2.bytes()));
            code.extend(rr(s.opcode, 0, 4));
            if let Some(r) = s.result {
                code.extend(store_reg(0, OFF_RESULT, r.bytes()));
            }
        }
        _ => {}
    }
    code.extend(ipm(R3));
    code.extend(rx(0x50, R3, 0, R1, OFF_CC));
    code.extend(mvc(4, R1, OFF_DONE, R4, 0x10));
    code
}

fn prologue() -> Vec<u8> {
    let mut code = larl(R4, ((CONSTANTS as i64 - CODE as i64) / 2) as i32);
    let cr0 = (CR0_SAVE - CONSTANTS) as u8;
    code.extend([0xB6, 0x00, R4 << 4, cr0]);
    code.extend([0x96, 0x04, R4 << 4, cr0 + 1]);
    code.extend([0xB7, 0x00, R4 << 4, cr0]);
    code
}

/// Where the case areas start: 0x10000, or the next page after the code when that runs past it.
fn area_base(cases: &[Case]) -> usize {
    let code: usize = prologue().len() + cases.iter().map(|c| case_code(&c.op, 0, 0).len()).sum::<usize>() + lpswe(R4, 0).len();
    (CODE + code).next_multiple_of(4096).max(AREAS)
}

pub fn image(cases: &[Case]) -> Vec<u8> {
    let base = area_base(cases);
    let mut mem = vec![0u8; (base + cases.len() * AREA).next_multiple_of(4096)];
    let mut put = |at: usize, bytes: &[u8]| mem[at..at + bytes.len()].copy_from_slice(bytes);
    let psw = |addr: usize| [&[0, 0, 0, 1, 0x80, 0, 0, 0][..], &(addr as u64).to_be_bytes()].concat();
    put(RESTART_PSW, &psw(CODE));
    put(PROGRAM_NEW_PSW, &psw(HANDLER));
    let handler = [
        mvc(4, R1, OFF_INTERRUPTION, 0, 0x8C).to_vec(),
        mvc(1, R1, OFF_DXC, 0, 0x93).to_vec(),
        lpswe(0, PROGRAM_OLD_PSW).to_vec(),
    ]
    .concat();
    put(HANDLER, &handler);
    put(WAIT_PSW, &[0x00, 0x02, 0x00, 0x01, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    put(DONE_AT, &DONE);
    let mut code = prologue();
    for (i, case) in cases.iter().enumerate() {
        let area = base + i * AREA;
        code.extend(case_code(&case.op, CODE + code.len(), area));
        let (op1, op2, mask) = operands(&case.op);
        put(area, &op1);
        put(area + usize::from(OFF_OP2), &op2);
        put(area + usize::from(OFF_MASK), &[ProgramMask::from_bits(mask).bits(), 0, 0, 0]);
    }
    code.extend(lpswe(R4, 0));
    put(CODE, &code);
    mem
}

pub fn config() -> String {
    "ARCHMODE z/Arch\nMAINSIZE 4\nNUMCPU 1\nCPUSERIAL 000001\nOSTAILOR QUIET\n".into()
}

pub fn script(image_path: &Path, dump_path: &Path, cases: usize) -> String {
    format!(
        "sysclear\narchlvl z/Arch\nloadcore \"{}\"\nruntest {}\nsavecore \"{}\" 0 *\nquit\n",
        image_path.display(),
        10 + cases / 100,
        dump_path.display()
    )
}

pub fn outcomes(dump: &[u8], cases: &[Case]) -> Result<Vec<Outcome>, String> {
    let base = area_base(cases);
    let mut unfinished = Vec::new();
    let mut out = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        let at = base + i * AREA;
        let area = dump.get(at..at + AREA).ok_or_else(|| format!("the dump ends at {:#x}, before case {i}", dump.len()))?;
        if area[usize::from(OFF_DONE)..usize::from(OFF_DONE) + 4] != DONE {
            unfinished.push(case.name.as_str());
            out.push(Outcome { result: Vec::new(), cc: None, interruption: None });
            continue;
        }
        let code = u16::from_be_bytes([area[usize::from(OFF_INTERRUPTION) + 2], area[usize::from(OFF_INTERRUPTION) + 3]]);
        let interruption = (code != 0).then_some(code);
        let cc_kept = interruption.is_none_or(|c| c == 0x0A);
        let cc = (sets_cc(&case.op) && cc_kept).then(|| area[usize::from(OFF_CC)] >> 4 & 3);
        let (offset, len) = result_at(&case.op);
        out.push(Outcome { result: area[usize::from(offset)..usize::from(offset) + len].to_vec(), cc, interruption });
    }
    if unfinished.is_empty() {
        Ok(out)
    } else {
        let shown: Vec<&str> = unfinished.iter().take(5).copied().collect();
        Err(format!("{} of {} cases never finished, first: {}", unfinished.len(), cases.len(), shown.join(", ")))
    }
}

fn interruption(e: ProgramCheck) -> Option<u16> {
    Some(u16::from(e.interruption_code()))
}

pub fn model(case: &Case) -> Outcome {
    match &case.op {
        Op::Hfp { inst, op1, op2, mask } => hfp_model(*inst, op1, op2, *mask),
        op => decimal_model(op),
    }
}

fn on_copy(mut a: Vec<u8>, f: impl FnOnce(&mut [u8]) -> Result<Option<Cc>, ProgramCheck>) -> Outcome {
    match f(&mut a) {
        Ok(cc) => Outcome { result: a, cc: cc.map(|c| c.0), interruption: None },
        Err(e) => Outcome { result: a, cc: None, interruption: interruption(e) },
    }
}

fn no_result(r: Result<Cc, ProgramCheck>) -> Outcome {
    match r {
        Ok(cc) => Outcome { result: Vec::new(), cc: Some(cc.0), interruption: None },
        Err(e) => Outcome { result: Vec::new(), cc: None, interruption: interruption(e) },
    }
}

fn decimal_model(op: &Op) -> Outcome {
    match op {
        Op::Pack { op1_len, op2 } => on_copy(vec![0; *op1_len], |a| decimal::pack(a, op2).map(|()| None)),
        Op::Unpk { op1_len, op2 } => on_copy(vec![0; *op1_len], |a| decimal::unpk(a, op2).map(|()| None)),
        Op::Zap { op1, op2 } => on_copy(op1.clone(), |a| decimal::zap(a, op2).map(Some)),
        Op::Ap { op1, op2 } => on_copy(op1.clone(), |a| decimal::ap(a, op2).map(Some)),
        Op::Sp { op1, op2 } => on_copy(op1.clone(), |a| decimal::sp(a, op2).map(Some)),
        Op::Mp { op1, op2 } => on_copy(op1.clone(), |a| decimal::mp(a, op2).map(|()| None)),
        Op::Dp { op1, op2 } => on_copy(op1.clone(), |a| decimal::dp(a, op2).map(|()| None)),
        Op::Srp { op1, shift, rounding } => on_copy(op1.clone(), |a| decimal::srp(a, *shift, *rounding).map(Some)),
        Op::Cp { op1, op2 } => no_result(decimal::cp(op1, op2)),
        Op::Tp { op } => no_result(decimal::tp(op)),
        Op::Cvb { op2 } => match decimal::cvb(op2) {
            Ok(v) => Outcome { result: v.to_be_bytes().to_vec(), cc: None, interruption: None },
            Err(e) => Outcome { result: Vec::new(), cc: None, interruption: interruption(e) },
        },
        Op::Cvd { value } => Outcome { result: decimal::cvd(*value).to_vec(), cc: None, interruption: None },
        Op::Hfp { .. } => unreachable!(),
    }
}

fn hfp_model(inst: HfpInst, op1: &[u8], op2: &[u8], mask: u8) -> Outcome {
    use HfpInst::*;
    let s = shape(inst);
    let pm = ProgramMask::from_bits(mask);
    let a = Hfp::from_bytes(s.p1, op1);
    let b = Hfp::from_bytes(s.p2, op2);
    let target = s.result.unwrap_or(s.p1);
    let computed: Result<(Option<Hfp>, Option<u8>), ProgramCheck> = match inst {
        Aer | Adr | Axr => a.add(b, pm).map(|r| (Some(r), Some(r.sign_cc().0))),
        Ser | Sdr | Sxr => a.sub(b, pm).map(|r| (Some(r), Some(r.sign_cc().0))),
        Aur | Awr => a.add_unnormalized(b, pm).map(|r| (Some(r), Some(r.sign_cc().0))),
        Mer | Mdr | Mxr | Mxdr => a.mul(b, target, pm).map(|r| (Some(r), None)),
        Der | Ddr => a.div(b, pm).map(|r| (Some(r), None)),
        Cer | Cdr => Ok((None, Some(Cc::from(a.compare(b)).0))),
        Her | Hdr => b.halve(pm).map(|r| (Some(r), None)),
        Ledr | Ldxr => b.round(target).map(|r| (Some(r), None)),
    };
    match computed {
        Ok((r, cc)) => Outcome { result: r.map(Hfp::to_bytes).unwrap_or_default(), cc, interruption: None },
        Err(e) => {
            let completes = matches!(e, ProgramCheck::HfpExponentOverflow | ProgramCheck::HfpExponentUnderflow | ProgramCheck::HfpSignificance);
            let kept = target.bytes();
            let result = if completes || kept > op1.len() || s.result.is_none() { Vec::new() } else { op1[..kept].to_vec() };
            Outcome { result, cc: None, interruption: interruption(e) }
        }
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn wide(&mut self) -> u128 {
        u128::from(self.next()) << 64 | u128::from(self.next())
    }
}

fn bytes(text: &str) -> Vec<u8> {
    unhex(text).expect("hex")
}

fn packed_of(value: i128, len: usize) -> Vec<u8> {
    let mut packed = vec![0; len];
    decimal::encode(&mut packed, Decimal { negative: value < 0, magnitude: value.unsigned_abs() }).expect("packed length");
    packed
}

fn random_packed(rng: &mut Rng, len: usize) -> Vec<u8> {
    let digits = 2 * len - 1;
    let used = 1 + rng.below(digits);
    let mut nibbles: Vec<u8> = (0..digits).map(|i| if digits - i <= used { rng.below(10) as u8 } else { 0 }).collect();
    if rng.below(16) == 0 {
        let i = rng.below(digits);
        nibbles[i] = 10 + rng.below(6) as u8;
    }
    let sign = if rng.below(16) == 0 { rng.below(10) as u8 } else { [0xC, 0xC, 0xD, 0xD, 0xF, 0xA, 0xB, 0xE][rng.below(8)] };
    nibbles.push(sign);
    nibbles.chunks(2).map(|p| p[0] << 4 | p[1]).collect()
}

fn random_zoned(rng: &mut Rng, len: usize) -> Vec<u8> {
    let mut out: Vec<u8> = (0..len).map(|_| 0xF0 | rng.below(10) as u8).collect();
    out[len - 1] = [0xC, 0xD, 0xF][rng.below(3)] << 4 | rng.below(10) as u8;
    if rng.below(8) == 0 {
        let i = rng.below(len);
        out[i] = rng.next() as u8;
    }
    out
}

fn hfp_bytes(p: Precision, negative: bool, characteristic: u8, fraction: u128) -> Vec<u8> {
    Hfp { precision: p, negative, characteristic, fraction }.to_bytes()
}

fn random_hfp(rng: &mut Rng, p: Precision) -> Vec<u8> {
    let d = p.digits() as usize;
    let characteristic = match rng.below(6) {
        0 => 0,
        1 => 1 + rng.below(3) as u8,
        2 => 127,
        3 => 126 - rng.below(3) as u8,
        _ => 0x30 + rng.below(0x20) as u8,
    };
    let mut fraction = rng.wide() & ((1u128 << (4 * d)) - 1);
    if rng.below(4) == 0 {
        fraction &= (1u128 << (4 * (d - rng.below(d)))) - 1;
    } else if fraction >> (4 * (d - 1)) == 0 {
        fraction |= (1 + rng.below(15) as u128) << (4 * (d - 1));
    }
    if rng.below(24) == 0 {
        fraction = 0;
    }
    hfp_bytes(p, rng.below(2) == 1, characteristic, fraction)
}

/// An edge-case fraction: hex digits, padded right with zeros, or with Fs when it ends in `+`.
fn edge_hfp(p: Precision, negative: bool, characteristic: u8, digits: &str) -> Vec<u8> {
    let d = p.digits() as usize;
    let (text, fill) = digits.strip_suffix('+').map_or((digits, '0'), |t| (t, 'F'));
    let padded: String = text.chars().chain(std::iter::repeat(fill)).take(d).collect();
    hfp_bytes(p, negative, characteristic, u128::from_str_radix(&padded, 16).expect("hex"))
}

fn hfp_case(name: String, inst: HfpInst, op1: Vec<u8>, op2: Vec<u8>, mask: u8) -> Case {
    Case { name, op: Op::Hfp { inst, op1, op2, mask } }
}

type Edge = (&'static str, bool, u8, &'static str, bool, u8, &'static str, u8);

const HFP_EDGES: [Edge; 34] = [
    ("zeros", false, 0, "0", false, 0, "0", 0),
    ("zeros-significance", false, 0, "0", false, 0, "0", 1),
    ("one-and-one", false, 0x41, "1", false, 0x41, "1", 0),
    ("cancel", false, 0x41, "1", true, 0x41, "1", 0),
    ("cancel-significance", false, 0x41, "1", true, 0x41, "1", 1),
    ("zero-fraction-with-characteristic", true, 0x41, "0", false, 0x41, "1", 0),
    ("underflow-difference", false, 0, "1", true, 0, "0F+", 0),
    ("underflow-difference-masked-on", false, 0, "1", true, 0, "0F+", 2),
    ("underflow-product", false, 1, "1", false, 1, "1", 0),
    ("underflow-product-masked-on", false, 1, "1", false, 1, "1", 2),
    ("overflow-sum", false, 0x7F, "F+", false, 0x7F, "F+", 0),
    ("overflow-sum-masks-on", false, 0x7F, "F+", false, 0x7F, "F+", 0xF),
    ("overflow-product", false, 0x7F, "1", false, 0x7F, "1", 0),
    ("divide-by-zero", false, 0x41, "1", false, 0x41, "0", 0),
    ("divide-zero-dividend", false, 0x41, "0", false, 0x41, "1", 0),
    ("divide-by-three", false, 0x41, "1", false, 0x41, "3", 0),
    ("divide-unnormalized", false, 0x41, "01", false, 0x41, "3", 0),
    ("unnormalized-sum", false, 0x41, "001", false, 0x41, "001", 0),
    ("unnormalized-cancel", false, 0x41, "000001", true, 0x41, "000001", 0),
    ("unnormalized-cancel-significance", false, 0x41, "000001", true, 0x41, "000001", 1),
    ("guard-digit", false, 0x41, "1", true, 0x40, "F+", 0),
    ("digit-shifted-past-guard", false, 0x42, "1", true, 0x40, "F+", 0),
    ("halve-smallest", false, 0, "1", false, 0, "1", 0),
    ("halve-smallest-underflow-on", false, 0, "1", false, 0, "1", 2),
    ("round-carries-into-exponent", false, 0, "0", false, 0x41, "F+", 0),
    ("round-carry-overflows", false, 0, "0", false, 0x7F, "F+", 0),
    ("round-no-carry", false, 0, "0", false, 0x41, "12345678", 0),
    ("compare-equal", false, 0x41, "1", false, 0x41, "1", 0),
    ("compare-low", false, 0x41, "1", false, 0x41, "2", 0),
    ("compare-high", false, 0x41, "2", false, 0x41, "1", 0),
    ("compare-unnormalized-equal", false, 0x41, "1", false, 0x42, "01", 0),
    ("compare-negative", true, 0x41, "1", false, 0x41, "1", 0),
    ("compare-zero-signs", true, 0, "0", false, 0, "0", 0),
    ("compare-guard", false, 0x41, "1", false, 0x40, "F+", 0),
];

fn decimal_edges() -> Vec<Case> {
    let mut cases = Vec::new();
    let mut add = |name: &str, op: Op| cases.push(Case { name: name.into(), op });
    let b = bytes;
    add("zap-negative-zero", Op::Zap { op1: b("12345C"), op2: b("0D") });
    add("zap-invalid-sign", Op::Zap { op1: b("12345C"), op2: b("1234") });
    add("zap-unsigned", Op::Zap { op1: b("00000C"), op2: b("123F") });
    add("zap-op1-not-validated", Op::Zap { op1: b("FFFFFF"), op2: b("012C") });
    add("zap-truncates", Op::Zap { op1: b("0C"), op2: b("12345C") });
    add("ap-negative-zero", Op::Ap { op1: b("0D"), op2: b("0C") });
    add("ap-overflow-short", Op::Ap { op1: b("99999C"), op2: b("00001C") });
    add("ap-overflow-negative", Op::Ap { op1: b("99999D"), op2: b("00001D") });
    add("ap-invalid-sign", Op::Ap { op1: b("123C"), op2: b("1234") });
    add("ap-invalid-digit", Op::Ap { op1: b("1A2C"), op2: b("123C") });
    add("ap-sign-flip", Op::Ap { op1: b("123C"), op2: b("124D") });
    add("ap-long", Op::Ap { op1: packed_of(1, 16), op2: b("5C") });
    add("sp-zero-result", Op::Sp { op1: b("123C"), op2: b("123C") });
    add("sp-invalid-digit", Op::Sp { op1: b("123C"), op2: b("1B3C") });
    add("sp-overflow", Op::Sp { op1: b("99999D"), op2: b("00001C") });
    add("mp-plain", Op::Mp { op1: b("000000123C"), op2: b("045C") });
    add("mp-leading-nonzero", Op::Mp { op1: b("0100123C"), op2: b("3C") });
    add("mp-multiplier-too-long", Op::Mp { op1: b("123C"), op2: b("0000123C") });
    add("mp-negative-zero-product", Op::Mp { op1: b("00000D"), op2: b("5C") });
    add("mp-negative", Op::Mp { op1: b("0000123D"), op2: b("5C") });
    add("dp-plain", Op::Dp { op1: b("0001234C"), op2: b("007C") });
    add("dp-negative", Op::Dp { op1: b("0001234D"), op2: b("007C") });
    add("dp-zero-divisor", Op::Dp { op1: b("000012345C"), op2: b("0C") });
    add("dp-quotient-too-long", Op::Dp { op1: b("1234567C"), op2: b("1C") });
    add("dp-negative-zero-remainder", Op::Dp { op1: b("0000000D"), op2: b("1C") });
    add("cp-negative-zero", Op::Cp { op1: b("0D"), op2: b("0C") });
    add("cp-unsigned-equals-plus", Op::Cp { op1: b("012F"), op2: b("012C") });
    add("cp-low", Op::Cp { op1: b("1C"), op2: b("2C") });
    add("cp-different-lengths", Op::Cp { op1: b("00000000001C"), op2: b("1C") });
    add("srp-left-overflow", Op::Srp { op1: b("12345C"), shift: 2, rounding: 0 });
    add("srp-left-fits", Op::Srp { op1: b("00123C"), shift: 2, rounding: 0 });
    add("srp-right-round5-carries", Op::Srp { op1: b("12345C"), shift: 0x3F, rounding: 5 });
    add("srp-right-round5-negative", Op::Srp { op1: b("12345D"), shift: 0x3F, rounding: 5 });
    add("srp-right-round5-no-carry", Op::Srp { op1: b("12344C"), shift: 0x3F, rounding: 5 });
    add("srp-right-everything", Op::Srp { op1: b("12345C"), shift: 0x20, rounding: 5 });
    add("srp-right-to-zero-negative", Op::Srp { op1: b("001D"), shift: 0x3F, rounding: 5 });
    add("srp-left-31", Op::Srp { op1: packed_of(1, 8), shift: 0x1F, rounding: 0 });
    add("srp-negative-zero", Op::Srp { op1: b("0D"), shift: 0, rounding: 0 });
    add("tp-valid", Op::Tp { op: b("123C") });
    add("tp-invalid-sign", Op::Tp { op: b("1234") });
    add("tp-invalid-digit", Op::Tp { op: b("1A3C") });
    add("tp-both", Op::Tp { op: b("1A34") });
    add("tp-one-byte", Op::Tp { op: b("5C") });
    add("tp-one-byte-bad-digit", Op::Tp { op: b("FC") });
    add("cvb-2147483647", Op::Cvb { op2: packed_of(2_147_483_647, 8).try_into().unwrap() });
    add("cvb-minimum", Op::Cvb { op2: packed_of(-2_147_483_648, 8).try_into().unwrap() });
    add("cvb-overflow", Op::Cvb { op2: packed_of(2_147_483_648, 8).try_into().unwrap() });
    add("cvb-underflow", Op::Cvb { op2: packed_of(-2_147_483_649, 8).try_into().unwrap() });
    add("cvb-negative-zero", Op::Cvb { op2: b("000000000000000D").try_into().unwrap() });
    add("cvb-invalid-sign", Op::Cvb { op2: b("0000000000000012").try_into().unwrap() });
    for (name, value) in [("minimum", i32::MIN), ("maximum", i32::MAX), ("zero", 0), ("minus-one", -1)] {
        add(&format!("cvd-{name}"), Op::Cvd { value });
    }
    add("pack-plain", Op::Pack { op1_len: 3, op2: b("F1F2F3F4") });
    add("pack-truncates", Op::Pack { op1_len: 1, op2: b("F1F2F3F4") });
    add("pack-sixteen-bytes", Op::Pack { op1_len: 9, op2: b("F1F2F3F4F5F6F7F8F9F0F1F2F3F4F5C6") });
    add("pack-letters", Op::Pack { op1_len: 3, op2: b("C1D2F3") });
    add("unpk-plain", Op::Unpk { op1_len: 3, op2: b("123C") });
    add("unpk-truncates", Op::Unpk { op1_len: 1, op2: b("123C") });
    add("unpk-extends", Op::Unpk { op1_len: 6, op2: b("012D") });
    cases
}

fn random_cases(rng: &mut Rng) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut add = |name: &str, i: usize, op: Op| cases.push(Case { name: format!("{name}-random-{i}"), op });
    for i in 0..40 {
        let len = |rng: &mut Rng| 1 + rng.below(16);
        let (l1, l2) = (len(rng), len(rng));
        add("pack", i, Op::Pack { op1_len: 1 + rng.below(9), op2: random_zoned(rng, l2) });
        let packed_len = 1 + rng.below(8);
        add("unpk", i, Op::Unpk { op1_len: l1, op2: random_packed(rng, packed_len) });
        add("zap", i, Op::Zap { op1: random_packed(rng, l1), op2: random_packed(rng, l2) });
        add("ap", i, Op::Ap { op1: random_packed(rng, l1), op2: random_packed(rng, l2) });
        add("sp", i, Op::Sp { op1: random_packed(rng, l1), op2: random_packed(rng, l2) });
        add("cp", i, Op::Cp { op1: random_packed(rng, l1), op2: random_packed(rng, l2) });
        let (multiplier, multiplicand) = (1 + rng.below(8), 0);
        let op1_len = multiplier + 1 + rng.below(16 - multiplier);
        let mut op1 = random_packed(rng, op1_len);
        if rng.below(8) != 0 {
            op1[..multiplier].fill(multiplicand);
        }
        add("mp", i, Op::Mp { op1, op2: random_packed(rng, multiplier) });
        let op1 = random_packed(rng, op1_len);
        add("dp", i, Op::Dp { op1, op2: random_packed(rng, multiplier) });
        add("srp", i, Op::Srp { op1: random_packed(rng, l1), shift: rng.below(64) as u8, rounding: rng.below(10) as u8 });
        add("tp", i, Op::Tp { op: random_packed(rng, l1) });
        let digits = 1 + rng.below(15);
        let value = (rng.wide() % 10u128.pow(digits as u32)) as i128;
        add("cvb", i, Op::Cvb { op2: packed_of(if rng.below(2) == 0 { -value } else { value }, 8).try_into().unwrap() });
        add("cvd", i, Op::Cvd { value: (rng.next() as i32) >> rng.below(32) });
    }
    cases
}

pub fn cases() -> Vec<Case> {
    let mut all = decimal_edges();
    for inst in ALL_HFP {
        let s = shape(inst);
        for (label, n1, c1, f1, n2, c2, f2, mask) in HFP_EDGES {
            all.push(hfp_case(format!("{inst:?}-{label}-mask{mask:X}"), inst, edge_hfp(s.p1, n1, c1, f1), edge_hfp(s.p2, n2, c2, f2), mask));
        }
    }
    let mut rng = Rng(0x853C_49E6_748F_EA9B);
    all.extend(random_cases(&mut rng));
    for inst in ALL_HFP {
        let s = shape(inst);
        for i in 0..40 {
            let op1 = random_hfp(&mut rng, s.p1);
            let op2 = if s.p1 == s.p2 && rng.below(6) == 0 {
                let mut near = Hfp::from_bytes(s.p2, &op1);
                near.fraction ^= rng.below(3) as u128;
                near.negative ^= rng.below(2) == 1;
                near.to_bytes()
            } else {
                random_hfp(&mut rng, s.p2)
            };
            let mask = rng.below(16) as u8;
            all.push(hfp_case(format!("{inst:?}-random-{i}"), inst, op1, op2, mask));
        }
    }
    all
}

pub fn run(dir: &Path, hercules: &str) -> Result<Vec<(Case, Outcome, Outcome)>, String> {
    let fail = |what: &str, e: std::io::Error| format!("{what}: {e}");
    fs::create_dir_all(dir).map_err(|e| fail(&dir.display().to_string(), e))?;
    let dir = fs::canonicalize(dir).map_err(|e| fail(&dir.display().to_string(), e))?;
    let cases = cases();
    let (image_path, dump_path, conf, rc, log) =
        (dir.join("image.bin"), dir.join("dump.bin"), dir.join("hercules.conf"), dir.join("hercules.rc"), dir.join("hercules.log"));
    fs::write(&image_path, image(&cases)).map_err(|e| fail("image.bin", e))?;
    fs::write(&conf, config()).map_err(|e| fail("hercules.conf", e))?;
    fs::write(&rc, script(&image_path, &dump_path, cases.len())).map_err(|e| fail("hercules.rc", e))?;
    let _ = fs::remove_file(&dump_path);
    let output = File::create(&log).map_err(|e| fail("hercules.log", e))?;
    let mut child = Command::new(hercules)
        .args(["-t2.0", "-f"])
        .arg(&conf)
        .arg("-r")
        .arg(&rc)
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(output.try_clone().map_err(|e| fail("hercules.log", e))?)
        .stderr(output)
        .spawn()
        .map_err(|e| fail(hercules, e))?;
    let started = Instant::now();
    loop {
        if child.try_wait().map_err(|e| fail(hercules, e))?.is_some() {
            break;
        }
        if started.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("{hercules} did not finish in 120 s; see {}", log.display()));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let dump = fs::read(&dump_path).map_err(|e| format!("{}: {e}; see {}", dump_path.display(), log.display()))?;
    let observed = outcomes(&dump, &cases).map_err(|e| format!("{e}; see {}", log.display()))?;
    Ok(cases.into_iter().zip(observed).map(|(case, seen)| {
        let expected = model(&case);
        (case, seen, expected)
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hex;

    #[test]
    fn instruction_encodings_are_exact() {
        assert_eq!(hex(&ss(0xFA, 7, 7, R1, 0, R1, 32)), "FA7710001020");
        assert_eq!(hex(&ss(0xF2, 4, 8, R1, 0, R1, 32)), "F24810001020");
        assert_eq!(hex(&ss(0xF0, 5, 5, R1, 0, 0, 0x3F)), "F0551000003F");
        assert_eq!(hex(&tp_inst(4, R1, 0)), "EB30100000C0");
        assert_eq!(hex(&rx(0x4F, R2, 0, R1, 32)), "4F201020");
        assert_eq!(hex(&rx(0x4E, R2, 0, R1, 80)), "4E201050");
        assert_eq!(hex(&rx(0x58, R2, 0, R1, 76)), "5820104C");
        assert_eq!(hex(&rx(0x50, R3, 0, R1, 64)), "50301040");
        assert_eq!(hex(&spm(R2)), "0420");
        assert_eq!(hex(&ipm(R3)), "B2220030");
        assert_eq!(hex(&lpswe(0, 0x150)), "B2B20150");
        assert_eq!(hex(&mvc(4, R1, 68, 0, 0x8C)), "D2031044008C");
        assert_eq!(hex(&larl(R1, 0x100)), "C01000000100");
        assert_eq!(hex(&rx(0x68, 0, 0, R1, 0)), "68001000");
        assert_eq!(hex(&rx(0x60, 0, 0, R1, 80)), "60001050");
        assert_eq!(hex(&rx(0x78, 0, 0, R1, 0)), "78001000");
        assert_eq!(hex(&rx(0x70, 0, 0, R1, 80)), "70001050");
    }

    #[test]
    fn hfp_register_instructions_encode_f0_f4() {
        let expected = [
            (HfpInst::Aer, "3A04"),
            (HfpInst::Adr, "2A04"),
            (HfpInst::Axr, "3604"),
            (HfpInst::Ser, "3B04"),
            (HfpInst::Sdr, "2B04"),
            (HfpInst::Sxr, "3704"),
            (HfpInst::Aur, "3E04"),
            (HfpInst::Awr, "2E04"),
            (HfpInst::Mer, "3C04"),
            (HfpInst::Mdr, "2C04"),
            (HfpInst::Mxr, "2604"),
            (HfpInst::Mxdr, "2704"),
            (HfpInst::Der, "3D04"),
            (HfpInst::Ddr, "2D04"),
            (HfpInst::Cer, "3904"),
            (HfpInst::Cdr, "2904"),
            (HfpInst::Her, "3404"),
            (HfpInst::Hdr, "2404"),
            (HfpInst::Ledr, "3504"),
            (HfpInst::Ldxr, "2504"),
        ];
        for (inst, text) in expected {
            assert_eq!(hex(&rr(shape(inst).opcode, 0, 4)), text, "{inst:?}");
        }
    }

    #[test]
    fn decimal_opcodes_are_the_architected_ones() {
        let ops = [
            (Op::Pack { op1_len: 1, op2: vec![0] }, 0xF2),
            (Op::Unpk { op1_len: 1, op2: vec![0] }, 0xF3),
            (Op::Zap { op1: vec![0], op2: vec![0] }, 0xF8),
            (Op::Cp { op1: vec![0], op2: vec![0] }, 0xF9),
            (Op::Ap { op1: vec![0], op2: vec![0] }, 0xFA),
            (Op::Sp { op1: vec![0], op2: vec![0] }, 0xFB),
            (Op::Mp { op1: vec![0], op2: vec![0] }, 0xFC),
            (Op::Dp { op1: vec![0], op2: vec![0] }, 0xFD),
        ];
        for (op, opcode) in ops {
            assert_eq!(decimal_opcode(&op), Some(opcode));
        }
    }

    #[test]
    fn an_add_case_is_larl_mask_load_instruction_and_bookkeeping() {
        let op = Op::Ap { op1: vec![0x1C; 8], op2: vec![0x2C; 8] };
        let code = case_code(&op, 0x2000, 0x10000);
        assert_eq!(hex(&code), "C010000070005820104C0420FA7710001020B222003050301040D20310784010");
    }

    #[test]
    fn image_places_psws_handler_and_operands() {
        let cases = vec![
            Case { name: "a".into(), op: Op::Ap { op1: vec![0x12, 0x3C], op2: vec![0x00, 0x1C] } },
            Case { name: "b".into(), op: Op::Hfp { inst: HfpInst::Aer, op1: vec![0x41, 0x10, 0, 0], op2: vec![0x41, 0x20, 0, 0], mask: 2 } },
        ];
        let mem = image(&cases);
        assert_eq!(hex(&mem[0x1A0..0x1B0]), "00000001800000000000000000002000");
        assert_eq!(hex(&mem[0x1D0..0x1E0]), "00000001800000000000000000001F00");
        assert_eq!(hex(&mem[0x1F00..0x1F00 + 16]), "D2031044008CD20010480093B2B20150");
        assert_eq!(hex(&mem[0x1E00..0x1E14]), "00020001800000000000000000000000C4D6D5C5");
        assert_eq!(hex(&mem[0x2000..0x2006]), "C040FFFFFF00");
        assert_eq!(mem.len() % 4096, 0);
        let base = area_base(&cases);
        assert_eq!(base, 0x10000);
        assert_eq!(hex(&mem[base..base + 2]), "123C");
        assert_eq!(hex(&mem[base + 32..base + 34]), "001C");
        assert_eq!(hex(&mem[base + 256..base + 260]), "41100000");
        assert_eq!(hex(&mem[base + 256 + 32..base + 256 + 36]), "41200000");
        assert_eq!(mem[base + 256 + 76], 0x02);
    }

    #[test]
    fn outcomes_decode_a_synthetic_dump() {
        let cases = vec![
            Case { name: "ap".into(), op: Op::Ap { op1: vec![0x12, 0x3C], op2: vec![0x00, 0x1C] } },
            Case { name: "pack".into(), op: Op::Pack { op1_len: 2, op2: vec![0xF1, 0xF2, 0xC3] } },
            Case { name: "bad".into(), op: Op::Ap { op1: vec![0x12, 0x3C], op2: vec![0x00, 0x1C] } },
        ];
        let base = area_base(&cases);
        let mut dump = vec![0u8; base + 3 * AREA];
        for i in 0..3 {
            dump[base + i * AREA + 120..base + i * AREA + 124].copy_from_slice(&DONE);
        }
        dump[base..base + 2].copy_from_slice(&[0x12, 0x4C]);
        dump[base + 64] = 0x20;
        dump[base + AREA..base + AREA + 2].copy_from_slice(&[0x01, 0x2C]);
        dump[base + AREA + 64] = 0x30;
        let out = outcomes(&dump, &cases[..2]).unwrap();
        assert_eq!(out[0], Outcome { result: vec![0x12, 0x4C], cc: Some(2), interruption: None });
        assert_eq!(out[1], Outcome { result: vec![0x01, 0x2C], cc: None, interruption: None });
        dump[base + 2 * AREA + 70..base + 2 * AREA + 72].copy_from_slice(&[0x00, 0x07]);
        dump[base + 2 * AREA + 64] = 0x10;
        let out = outcomes(&dump, &cases).unwrap();
        assert_eq!(out[2].interruption, Some(7));
        assert_eq!(out[2].cc, None);
        dump[base + 2 * AREA + 120] = 0;
        assert!(outcomes(&dump, &cases).unwrap_err().contains("bad"));
    }

    #[test]
    fn the_model_answers_from_zarch() {
        let ap = Case { name: "ap".into(), op: Op::Ap { op1: bytes("123C"), op2: bytes("001C") } };
        assert_eq!(model(&ap), Outcome { result: bytes("124C"), cc: Some(2), interruption: None });
        let dp = Case { name: "dp".into(), op: Op::Dp { op1: bytes("000012345C"), op2: bytes("0C") } };
        assert_eq!(model(&dp).interruption, Some(0x0B));
        let one = edge_hfp(Precision::Short, false, 0x41, "1");
        let sum = hfp_case("s".into(), HfpInst::Aer, one.clone(), one.clone(), 0);
        assert_eq!(model(&sum), Outcome { result: bytes("41200000"), cc: Some(2), interruption: None });
        let cancel = hfp_case("c".into(), HfpInst::Ser, one.clone(), one.clone(), 1);
        assert_eq!(model(&cancel).interruption, Some(0x0E));
        let half = hfp_case("h".into(), HfpInst::Her, one.clone(), one, 0);
        assert_eq!(model(&half).result, bytes("40800000"));
        let unnormalized = edge_hfp(Precision::Short, false, 0x41, "001");
        let half = hfp_case("h".into(), HfpInst::Her, unnormalized.clone(), unnormalized, 0);
        assert_eq!(model(&half).result, bytes("3E800000"));
    }

    #[test]
    fn cases_are_deterministic_and_every_one_encodes() {
        let all = cases();
        assert_eq!(all.len(), cases().len());
        assert!(all.len() > 1000);
        assert_eq!(all[0].name, cases()[0].name);
        assert!(all.iter().filter(|c| c.name.starts_with("Aer-random")).count() == 40);
        let mem = image(&all);
        assert_eq!(mem.len(), (area_base(&all) + all.len() * AREA).next_multiple_of(4096));
    }
}
