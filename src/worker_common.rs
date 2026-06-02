use std::fmt;
use std::fmt::Formatter;
use std::ops::Add;
use chrono::{DateTime, TimeDelta, Timelike, Utc};
use foxess::FoxError;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use crate::{DEBUG_MODE, MANUAL_DAY};
use crate::manager_files::{get_schedule_for_date, load_scheduled_blocks, FileManagerError};
use crate::manual::ManualDaysError;
use crate::scheduler_common::SchedulingError;

#[derive(Serialize, Deserialize, Clone)]
pub struct ImportSchedule {
    pub blocks: Vec<Block>,
    pub schedule_id: i64,
}

/// Size of the smallest block possible in minutes
pub const BLOCK_UNIT_SIZE: i64 = 15;

/// Available block types
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum BlockType {
    Charge,
    Hold,
    Use,
    Unknown,
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for BlockType {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            BlockType::Charge => write!(f, "Charge "),
            BlockType::Hold   => write!(f, "Hold   "),
            BlockType::Use    => write!(f, "Use    "),
            BlockType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Block status
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum Status {
    Waiting,
    Started,
    Full(FullAt),
    Error,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct FullAt {
    pub time: DateTime<Utc>,
    pub soc: usize,
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for Status {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Status::Waiting => write!(f, "Waiting  "),
            Status::Started => write!(f, "Started  "),
            Status::Full(full_at) => write!(f, "Full: {:>3} {:02}:{:02}", full_at.soc, full_at.time.hour(), full_at.time.minute()),
            Status::Error   => write!(f, "Error    "),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Block {
    pub(crate) block_id: usize,
    pub block_type: BlockType,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub cost: f64,
    pub true_soc_in: Option<usize>,
    pub soc_in: usize,
    pub soc_out: usize,
    pub status: Status,
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for Block {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let length = self.end_time.add(TimeDelta::minutes(BLOCK_UNIT_SIZE)) - self.start_time;

        // Build base output
        let output = format!("{} -> {:>02}:{:>02} - Length: {:>02}:{:>02}: SocIn {:>3}, SocOut {:>3}, True SocIn: {:>3}, Cost {:>5.2} ",
                             self.block_type,
                             self.start_time.hour(), self.start_time.minute(),
                             length.num_hours(), length.num_minutes() - length.num_hours() * 60,
                             self.soc_in, self.soc_out,
                             self.true_soc_in.unwrap_or(0), self.cost);

        write!(f, "{}", output)
    }
}

/// Check if we are in debug mode or manual day
///
pub fn is_manual_debug() -> anyhow::Result<bool, WorkerError> {
    let debug_mode = *DEBUG_MODE
        .read()
        .map_err(|e| WorkerError::LockPoison(e.to_string()))?;

    let manual_day = *MANUAL_DAY
        .read()
        .map_err(|e| WorkerError::LockPoison(e.to_string()))?;

    Ok(debug_mode || manual_day)
}

pub fn import_schedule_from_file(schedule_dir: &str, date_time: DateTime<Utc>) -> Result<Option<ImportSchedule>, WorkerError> {
    let loaded_schedule = match load_scheduled_blocks(schedule_dir, date_time)? {
        Some(schedule) => Some(schedule),
        None => get_schedule_for_date(schedule_dir, date_time)?,
    };

    Ok(loaded_schedule)
}

#[derive(Error, Debug)]
pub enum WorkerError {
    #[error(transparent)]
    FileManager(#[from] FileManagerError),
    #[error(transparent)]
    SkipDay(#[from] ManualDaysError),
    #[error(transparent)]
    FoxESS(#[from] FoxError),
    #[error(transparent)]
    Scheduling(#[from] SchedulingError),
    #[error("lock poison error: {0}")]
    LockPoison(String),
}
