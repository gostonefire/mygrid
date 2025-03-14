use std::fmt;
use std::fmt::Formatter;
use chrono::{Local, RoundingError};
use crate::backup::BackupError;
use crate::manager_fox_cloud::FoxError;
use crate::manager_mail::MailError;
use crate::scheduling::{Block, SchedulingError};

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
