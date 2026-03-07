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
    pub mode_scheduler: bool,
    pub soc_kwh: f64,
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
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for BlockType {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            BlockType::Charge => write!(f, "Charge"),
            BlockType::Hold   => write!(f, "Hold  "),
            BlockType::Use    => write!(f, "Use   "),
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
    pub charge_in: f64,
    pub charge_out: f64,
    pub true_soc_in: Option<usize>,
    pub soc_in: usize,
    pub soc_out: usize,
    pub(crate) soc_kwh: f64,
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

impl Block {
    /// Updates the status of the block
    ///
    /// # Arguments
    ///
    /// * 'status' - the status to update with
    ///  * 'soc' - current soc
    pub fn update_block_status(&mut self, status: Status, soc: Option<u8>) {
        if self.block_type == BlockType::Charge {
            if let Status::Full(full_at) = &status {
                self.soc_out = full_at.soc;
                self.charge_out = (full_at.soc - 10) as f64 * self.soc_kwh;
            }
        }
        self.status = status;
        if let Some(soc) = soc {
            self.true_soc_in = Some(soc as usize);
        }
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

pub fn import_schedule(schedule_dir: &str, date_time: DateTime<Utc>, as_mode_scheduler: bool) -> Result<Option<ImportSchedule>, WorkerError> {
    let loaded_schedule = load_scheduled_blocks(schedule_dir, date_time)?
        .or(get_schedule_for_date(schedule_dir, date_time)?);

    if as_mode_scheduler && (loaded_schedule.is_none() || loaded_schedule.as_ref().is_some_and(|s| !s.mode_scheduler)) {
        return Err(WorkerError::IsManualSchedule);
    }

    if !as_mode_scheduler && loaded_schedule.as_ref().is_some_and(|s| s.mode_scheduler) {
        return Err(WorkerError::IsModeSchedule);
    }

    Ok(loaded_schedule)
}

#[derive(Error, Debug)]
pub enum WorkerError {
    #[error("schedule is for mode scheduler")]
    IsModeSchedule,
    #[error("schedule is for manual scheduler")]
    IsManualSchedule,
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
    #[error("error while set charge mode: {0}")]
    SetCharge(String),
    #[error("error while set hold mode: {0}")]
    SetHold(String),
    #[error("error while set use mode: {0}")]
    SetUse(String),
    #[error("other error: {0}")]
    Other(&'static str),
}
