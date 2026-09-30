//! The typed abend code an `AbendText` holds, as exec/src/abend.rs and files.rs define it.

use crate::codec_enum;
use zarch::check::ProgramCheck;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AbendCode {
    Check(ProgramCheck),
    Protection,
    ModuleNotFound,
    Io(FileStatus),
    Cics(String),
    User(String),
    Ironwork,
    Exec,
    Sql,
    SqlReplay,
    Java,
    Signal(Signal),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    DivideByZero,
    StopRun,
    GoBack,
    SortStopped,
    ClosedOutput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileStatus {
    Success,
    SuccessDuplicate,
    SuccessWrongLength,
    SuccessOptional,
    AtEnd,
    RelativeKeyOverflow,
    SequenceError,
    DuplicateKey,
    NotFound,
    BoundaryViolation,
    PermanentError,
    FileNotFound,
    OpenModeUnsupported,
    AlreadyOpen,
    NotOpen,
    NoPriorRead,
    RecordLengthChanged,
    NoNextRecord,
    NotOpenInput,
    NotOpenOutput,
    NotOpenInputOutput,
}

codec_enum!(AbendCode {
    Check(check) = 0,
    Protection = 1,
    ModuleNotFound = 2,
    Io(status) = 3,
    Cics(code) = 4,
    User(code) = 5,
    Ironwork = 6,
    Exec = 7,
    Sql = 8,
    SqlReplay = 9,
    Java = 10,
    Signal(signal) = 11,
});
codec_enum!(Signal { DivideByZero = 0, StopRun = 1, GoBack = 2, SortStopped = 3, ClosedOutput = 4 });
codec_enum!(FileStatus {
    Success = 0,
    SuccessDuplicate = 1,
    SuccessWrongLength = 2,
    SuccessOptional = 3,
    AtEnd = 4,
    RelativeKeyOverflow = 5,
    SequenceError = 6,
    DuplicateKey = 7,
    NotFound = 8,
    BoundaryViolation = 9,
    PermanentError = 10,
    FileNotFound = 11,
    OpenModeUnsupported = 12,
    AlreadyOpen = 13,
    NotOpen = 14,
    NoPriorRead = 15,
    RecordLengthChanged = 16,
    NoNextRecord = 17,
    NotOpenInput = 18,
    NotOpenOutput = 19,
    NotOpenInputOutput = 20,
});
