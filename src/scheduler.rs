use std::{fmt, fs};
use std::fmt::Formatter;
use std::ops::Add;
use std::path::PathBuf;
use chrono::{DateTime, DurationRound, NaiveDateTime, TimeDelta, Timelike, Utc};
use log::warn;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use crate::errors::SchedulingError;

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
    Full(usize),
    Error,
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for Status {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Status::Waiting => write!(f, "Waiting  "),
            Status::Started => write!(f, "Started  "),
            Status::Full(soc) => write!(f, "Full: {:>3}", soc),
            Status::Error   => write!(f, "Error    "),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Block {
    block_id: usize,
    pub block_type: BlockType,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    //pub start_hour: usize,
    //pub start_minute: usize,
    //pub end_hour: usize,
    //pub end_minute: usize,
    //size: usize,
    pub cost: f64,
    pub charge_in: f64,
    pub charge_out: f64,
    pub true_soc_in: Option<usize>,
    pub soc_in: usize,
    pub soc_out: usize,
    soc_kwh: f64,
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
            if let Status::Full(soc) = status {
                self.soc_out = soc;
                self.charge_out = (soc - 10) as f64 * self.soc_kwh;
            }
        }
        self.status = status;
        if let Some(soc) = soc {
            self.true_soc_in = Some(soc as usize);
        }
    }
}


/// Struct representing the block schedule from the current hour and forward
pub struct Schedule {
    schedule_dir: String,
    soc_kwh: f64,
    pub blocks: Vec<Block>,
}

impl Schedule {
    /// Creates a new Schedule without scheduling
    ///
    /// # Arguments
    ///
    /// * 'schedule_dir' - directory where schedule files are held
    /// * 'soc_kwh' - kWh per soc unit
    /// * 'schedule_blocks' - any existing schedule blocks
    pub fn new(schedule_dir: &str, soc_kwh: f64, schedule_blocks: Option<Vec<Block>>) -> Schedule {
        Schedule {
            //date_time: Default::default(),
            schedule_dir: schedule_dir.to_string(),
            soc_kwh,
            blocks: schedule_blocks.unwrap_or(Vec::new()),
        }
    }

    /// Returns a block id of a block identified by hour
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the time to get a block for
    /// * 'with_fallback' - if set to true, and if there is no block for the given time, a fallback schedule is created
    pub fn get_block_by_time(&mut self, date_time: DateTime<Utc>, with_fallback: bool) -> Option<usize> {
        let date_hour = date_time.duration_trunc(TimeDelta::minutes(BLOCK_UNIT_SIZE)).unwrap();
        for b in self.blocks.iter() {
            if b.start_time <= date_hour && b.end_time >= date_hour {
                return Some(b.block_id);
            }
        }

        if with_fallback {
            self.blocks = get_fallback_schedule(self.soc_kwh);
            Some(0)
        } else {
            None
        }
    }

    /// Returns a mutable block identified by its block id
    ///
    /// # Arguments
    ///
    /// * 'block_ld' - id of the block
    pub fn get_block_by_id(&mut self, block_id: usize) -> Option<&mut Block> {
        self.blocks.iter_mut().find(|b| b.block_id == block_id)
    }

    /// Check if it is time to update to next step in schedule
    ///
    /// # Arguments
    ///
    /// * 'block_id' - id of the block to check
    /// * 'date_time' - the date time the block is valid for
    pub fn is_update_time(&self, block_id: usize, date_time: DateTime<Utc>) -> bool {
        let date_hour = date_time.duration_trunc(TimeDelta::minutes(BLOCK_UNIT_SIZE)).unwrap();
        let block = self.blocks.iter().find(|b| b.block_id == block_id);

        block.is_none_or(|b| (b.start_time > date_hour || b.end_time < date_hour) ||
            (b.start_time <= date_hour && b.end_time >= date_hour && b.status == Status::Waiting))
    }

    /// Check if we are in an active charge block and charging is still ongoing
    ///
    /// # Arguments
    ///
    /// * 'block_id' - id of the block to check
    /// * 'date_time' - the date time the block is valid for
    pub fn is_active_charging(&self, block_id: usize, date_time: DateTime<Utc>) -> bool {
        let date_hour = date_time.duration_trunc(TimeDelta::minutes(BLOCK_UNIT_SIZE)).unwrap();
        let block = self.blocks.iter().find(|b| b.block_id == block_id);

        block.is_some_and(|b| b.start_time <= date_hour && b.end_time >= date_hour
            && b.block_type == BlockType::Charge && b.status == Status::Started)
    }

    /// Updates scheduling from scheduler output.
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the date time to stamp on the schedule
    pub fn update_scheduling(&mut self, date_time: DateTime<Utc>) -> Result<(), SchedulingError> {
        let path = format!("{}*_schedule.json", self.schedule_dir);
        for entry in glob::glob(&path)? {
            match entry {
                Ok(p) => {
                    let (schedule_start, schedule_end) = get_schedule_time(&p)?;

                    if date_time >= schedule_start && date_time < schedule_end {
                        let json = fs::read_to_string(p).unwrap();
                        let blocks: Vec<Block> = serde_json::from_str(&json)?;
                        self.blocks = blocks;
                        return Ok(());
                    }
                }
                Err(e) => warn!("{:?}", e),
            }
        }
        
        Ok(())
    }
}

/// Returns the date time representation of the schedule start time which is 
/// encoded in the schedule file name
/// 
/// # Arguments
/// 
/// * 'path_buf' - the full path to the schedule file
fn get_schedule_time(path_buf: &PathBuf) -> Result<(DateTime<Utc>, DateTime<Utc>), SchedulingError> {
    let file_name = path_buf.file_name()
        .ok_or("Error in schedule file name")?
        .to_str()
        .ok_or("Illegal character in schedule file name")?;
    
    if file_name.len() != 39 {
        Err("malformed schedule file name")?
    } else {
        let start = NaiveDateTime::parse_from_str(&file_name[0..12], "%Y%m%d%H%M")?.and_utc();
        let end = NaiveDateTime::parse_from_str(&file_name[13..25], "%Y%m%d%H%M")?.and_utc();
        Ok((start, end))
    }
}

/// Creates a fallback schedule of block
/// 
/// # Arguments
/// 
/// * 'soc_kwh' - kWh per soc unit
fn get_fallback_schedule(soc_kwh: f64) -> Vec<Block> {
    let block = Block {
        block_id: 0,
        block_type: BlockType::Use,
        start_time: Default::default(),
        end_time: Default::default(),
        //start_hour: 0,
        //start_minute: 0,
        //end_hour: 0,
        //end_minute: 0,
        //size: 0,
        cost: 0.0,
        charge_in: 10.0 * soc_kwh,
        charge_out: 10.0 * soc_kwh,
        true_soc_in: None,
        soc_in: 10,
        soc_out: 10,
        soc_kwh,
        status: Status::Waiting,
    };
    
    vec![block]
}