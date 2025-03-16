use std::fmt;
use std::fmt::Formatter;
use std::sync::{PoisonError, RwLockReadGuard, RwLockWriteGuard};
use chrono::{Local, RoundingError};
use crate::manager_fox_cloud::errors::FoxError;
use crate::manager_mail::errors::MailError;
use crate::manager_nordpool::errors::NordPoolError;
use crate::manager_smhi::errors::SMHIError;
use crate::scheduling::Block;


/// Error depicting errors that occur during initialization of the main program
///
pub struct MyGridInitError(pub String);

impl fmt::Display for MyGridInitError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "MyGridInitError: {}", self.0)
    }
}
impl From<SchedulingError> for MyGridInitError {
    fn from(e: SchedulingError) -> Self {
        MyGridInitError(e.to_string())
    }
}
impl From<BackupError> for MyGridInitError {
    fn from(e: BackupError) -> Self {
        MyGridInitError(e.to_string())
    }
}
impl From<MailError> for MyGridInitError {
    fn from(e: MailError) -> Self { MyGridInitError(e.to_string()) }
}
impl From<PoisonError<RwLockReadGuard<'_, bool>>> for MyGridInitError {
    fn from(e: PoisonError<RwLockReadGuard<'_, bool>>) -> Self { MyGridInitError(e.to_string()) }
}
impl From<PoisonError<RwLockWriteGuard<'_, bool>>> for MyGridInitError {
    fn from(e: PoisonError<RwLockWriteGuard<'_, bool>>) -> Self { MyGridInitError(e.to_string()) }
}


/// Error depicting errors that occur while running the main program
///
pub struct MyGridWorkerError {
    msg: String,
    block: Option<Block>,
}

impl MyGridWorkerError {
    pub fn new(msg: String, block: &Block) -> MyGridWorkerError {
        MyGridWorkerError {
            msg: msg.to_string(),
            block: Some(block.clone()),
        }
    }
}
impl fmt::Display for MyGridWorkerError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let report_time = format!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
        let caption = format!("{} MyGridWorkerError ", report_time);
        write!(f, "{:=<137}\n", caption)?;
        write!(f, "{}\n", self.msg)?;
        if let Some(block) = &self.block {
            write!(f, "Block:\n{}", block.to_string())?;
        }

        Ok(())
    }
}
impl From<SchedulingError> for MyGridWorkerError {
    fn from(e: SchedulingError) -> Self {
        MyGridWorkerError { msg: e.to_string(), block: None }
    }
}
impl From<BackupError> for MyGridWorkerError {
    fn from(e: BackupError) -> Self {
        MyGridWorkerError { msg: e.to_string(), block: None }
    }
}
impl From<FoxError> for MyGridWorkerError {
    fn from(e: FoxError) -> Self {
        MyGridWorkerError { msg: e.to_string(), block: None }
    }
}
impl From<RoundingError> for MyGridWorkerError {
    fn from(e: RoundingError) -> Self { MyGridWorkerError { msg: e.to_string(), block: None }}
}
impl From<PoisonError<RwLockReadGuard<'_, bool>>> for MyGridWorkerError {
    fn from(e: PoisonError<RwLockReadGuard<'_, bool>>) -> Self { MyGridWorkerError { msg: e.to_string(), block: None }}
}
impl From<SkipError> for MyGridWorkerError {
    fn from(e: SkipError) -> Self { MyGridWorkerError { msg: e.to_string(), block: None }}
}



/// Error depicting errors that occur while doing backup operations
///
pub struct BackupError(String);

impl fmt::Display for BackupError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "BackupError: {}", self.0)
    }
}
impl From<std::io::Error> for BackupError {
    fn from(e: std::io::Error) -> Self {
        BackupError(e.to_string())
    }
}
impl From<serde_json::Error> for BackupError {
    fn from(e: serde_json::Error) -> Self {
        BackupError(e.to_string())
    }
}
impl From<glob::PatternError> for BackupError {
    fn from(e: glob::PatternError) -> Self {
        BackupError(e.to_string())
    }
}
impl From<glob::GlobError> for BackupError {
    fn from(e: glob::GlobError) -> Self {
        BackupError(e.to_string())
    }
}
impl From<RoundingError> for BackupError {
    fn from(e: RoundingError) -> Self {
        BackupError(e.to_string())
    }
}
impl From<FoxError> for BackupError {
    fn from(e: FoxError) -> Self {
        BackupError(e.to_string())
    }
}


/// Error depicting errors that occur while creating and managing schedules
///
pub struct SchedulingError(pub String);

impl fmt::Display for SchedulingError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "SchedulingError: {}", self.0)
    }
}
impl From<&str> for SchedulingError {
    fn from(e: &str) -> Self {
        SchedulingError(e.to_string())
    }
}
impl From<FoxError> for SchedulingError {
    fn from(e: FoxError) -> Self {
        SchedulingError(e.to_string())
    }
}
impl From<NordPoolError> for SchedulingError {
    fn from(e: NordPoolError) -> Self {
        SchedulingError(e.to_string())
    }
}
impl From<SMHIError> for SchedulingError {
    fn from(e: SMHIError) -> Self {
        SchedulingError(e.to_string())
    }
}
impl From<BackupError> for SchedulingError {
    fn from(e: BackupError) -> Self {
        SchedulingError(e.to_string())
    }
}


/// Error depicting errors that occur while doing skip file operations
///
pub struct SkipError(String);

impl fmt::Display for SkipError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "SkipError: {}", self.0)
    }
}
impl From<std::io::Error> for SkipError {
    fn from(e: std::io::Error) -> Self {
        SkipError(e.to_string())
    }
}
impl From<serde_json::Error> for SkipError {
    fn from(e: serde_json::Error) -> Self {
        SkipError(e.to_string())
    }
}
impl From<PoisonError<RwLockReadGuard<'_, bool>>> for SkipError {
    fn from(e: PoisonError<RwLockReadGuard<'_, bool>>) -> Self { SkipError(e.to_string()) }
}
impl From<PoisonError<RwLockWriteGuard<'_, bool>>> for SkipError {
    fn from(e: PoisonError<RwLockWriteGuard<'_, bool>>) -> Self { SkipError(e.to_string()) }
}

