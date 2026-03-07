use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use crate::worker_common::{Block, BlockType, ImportSchedule, Status, BLOCK_UNIT_SIZE};
use crate::manager_files::get_schedule_for_date;
use crate::scheduler_common::SchedulingError;

/// Struct representing the block schedule from the current hour and forward
pub struct Schedule {
    schedule_dir: String,
    pub soc_kwh: f64,
    pub schedule_id: i64,
    pub blocks: Vec<Block>,
}

impl Schedule {
    /// Creates a new Schedule without scheduling
    ///
    /// # Arguments
    ///
    /// * 'schedule_dir' - directory where schedule files are held
    /// * 'default_soc_kwh' - default kWh per soc unit (if no blocks are provided in the first time schedule)
    /// * 'schedule_blocks' - any existing schedule blocks
    pub fn new(schedule_dir: &str, default_soc_kwh: f64, schedule_blocks: Option<ImportSchedule>) -> Schedule {
        Schedule {
            schedule_dir: schedule_dir.to_string(),
            soc_kwh: schedule_blocks.as_ref().map(|b| b.soc_kwh).unwrap_or(default_soc_kwh),
            schedule_id: schedule_blocks.as_ref().map(|b| b.schedule_id).unwrap_or(0),
            blocks: schedule_blocks.map(|b| b.blocks).unwrap_or(Vec::new()),
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
    /// * 'date_time' - the date time the schedule needs to include
    pub fn update_scheduling(&mut self, date_time: DateTime<Utc>) -> Result<Option<ImportSchedule>, SchedulingError> {
        if let Some(import_schedule) = get_schedule_for_date(&self.schedule_dir, date_time)?
        {
            if import_schedule.mode_scheduler{
                return Ok(Some(import_schedule));
            }
            self.blocks = import_schedule.blocks;
            self.soc_kwh = import_schedule.soc_kwh;
            self.schedule_id = import_schedule.schedule_id;
        }

        Ok(None)
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