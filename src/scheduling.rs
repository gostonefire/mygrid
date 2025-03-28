use std::collections::{HashSet};
use std::{fmt};
use std::fmt::Formatter;
use std::ops::Add;
use std::thread;
use chrono::{DateTime, Local, TimeDelta, Timelike};
use serde::{Deserialize, Serialize};
use crate::consumption::Consumption;
use crate::manager_nordpool::NordPool;
use crate::manager_smhi::SMHI;
use crate::production::PVProduction;
use crate::{retry, wrapper, LAT, LONG};
use crate::backup::save_backup;
use crate::errors::SchedulingError;

/// Time needed to fully charge batteries from SoC 10% to SoC 100%
const CHARGE_LEN: u8 = 3;

/// Time available in batteries to supply power when in self use mode
const USE_LEN: u8 = 5;

/// The battery capacity in watts divided by the max SoC (State of Charge). This represents
/// roughly how much each percentage of the SoC is in terms of power (Wh)
const SOC_CAPACITY_W: f64 = 16590.0 / 100.0;

/// The inverter round-trip efficiency.
/// If a charge price is x, then the discharge tariff must be equal or higher
/// than x / INVERTER_EFFICIENCY
const INVERTER_EFFICIENCY: f64 = 0.8;

/// The min tariff price an hour must meet or exceed for it to be part of a use block
/// regardless of tariff price during battery charging. This to avoid chasing mosquito with
/// an elephant gun, better save battery from charge cycles if it doesn't give money back.
const MIN_USE_TARIFF: f64 = 0.5;



/// Available block types
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Waiting,
    Started,
    Missed,
    Full,
    Error,
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for Status {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Status::Waiting => write!(f, "Waiting"),
            Status::Started => write!(f, "Started"),
            Status::Missed  => write!(f, "Missed "),
            Status::Full    => write!(f, "Full   "),
            Status::Error   => write!(f, "Error  "),
        }
    }
}

/// Represents one block in a schedule
#[derive(Clone, Serialize, Deserialize)]
pub struct Block {
    pub block_type: BlockType,
    pub max_min_soc: u8,
    pub max_soc: u8,
    pub start_hour: u8,
    pub end_hour: u8,
    pub mean_price: f64,
    pub hour_price: Vec<f64>,
    pub is_updated: bool,
    pub status: Status,
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for Block {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        // Divide all prices in chunks of max 8 prices
        let price_chunks: Vec<Vec<f64>> = self.hour_price
            .chunks(8)
            .map(|c| c.to_vec())
            .collect();

        // Build base output
        let mut output = format!("{} - {} -> {:>2} - {:>2}: maxMinSoc {:<3}, maxSoc {:<3}, updated: {:>5}, price {:<7.3} ",
                                 self.status, self.block_type, self.start_hour, self.end_hour,
                                 self.max_min_soc, self.max_soc, self.is_updated, self.mean_price);

        // Build and attach text line for each chunk to the output string
        let chunks = &mut price_chunks.iter().enumerate();
        while let Some((i, chunk)) = chunks.next() {
            let mut line = "[".to_string();
            for price in chunk {
                line.push_str(&format!("{:>6.2}", price));
            }
            line.push_str(" ]");

            // First line should follow immediately after the base output, rest of lines
            // shall start after 70 spaces from beginning
            if i == 0 {
                output.push_str(&line);
            } else {
                output.push_str(&format!("\n{:>86}{}", "", line));
            }
        }

        write!(f, "{}", output)
    }
}

/// Struct representing one day's block schedule
#[derive(Serialize, Deserialize, Clone)]
pub struct Schedule {
    pub date: DateTime<Local>,
    pub blocks: Vec<Block>,
    pub tariffs: [f64;24],
}

impl Schedule {
    /// Creates a new empty schedule with its date set to yesterday
    pub fn new() -> Schedule {
        Schedule {
            date: Local::now().add(TimeDelta::days(-1)),
            blocks: Vec::new(),
            tariffs: [0.0; 24],
        }
    }

    /// Creates a schedule over one day given that days tariffs.
    ///
    /// It divides the day over three segments and finds the best block for charging
    /// that also has a suitable use block. If none is found in segment one it continues to
    /// search in segment 2 and finally in segment three.
    ///
    /// Depending on how many use blocks were found after a charge block it either tries to find a new
    /// charge block with open end to end of day, or a charge block between two use blocks.
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - tariffs from NordPool for the day to create schedule for
    /// * 'date_time' - date and time to stamp the schedule with
    pub fn from_tariffs(tariffs: &Vec<f64>, date_time: DateTime<Local>) -> Result<Schedule, SchedulingError> {
        let mut schedule = Schedule { date: date_time, blocks: Vec::new(), tariffs: [0.0;24] };
        schedule.tariffs_to_array(tariffs)?;

        let segments: [(u8,u8);3] = [(0,8 - CHARGE_LEN), (8, 16 - CHARGE_LEN), (16, 24 - CHARGE_LEN)];

        // Find the best charge block with following use block(s) where mean price for a use block is
        // at least 25% more expensive (to factor in inverter/battery efficiency factor of
        // roughly 80% efficiency full circle). The day is divided in three segments, but only
        // the charge block is affected by that boundary.
        let mut blocks: Vec<Block> = Vec::new();
        for s in segments.iter() {
            let charge_block = Self::get_charge_block(&tariffs, CHARGE_LEN, s.0, s.1);
            let min_price = (charge_block.mean_price / INVERTER_EFFICIENCY).max(MIN_USE_TARIFF);
            blocks = Self::get_use_block(&tariffs, min_price, USE_LEN, charge_block.end_hour + 1, 23);
            if !blocks.is_empty() {
                schedule.blocks.push(charge_block);
                schedule.blocks.push(blocks[0].clone());
                break;
            }
        }

        // If we did find a charge block followed by use block(s) we continue to find more charge block
        // with following use block(s).
        while !blocks.is_empty() {
            // If there was only one use block found, we try to find a new charge block with open end
            // to end of day, with following use block(s)
            if blocks.len() == 1 {
                let charge_block = Self::get_charge_block(&tariffs, CHARGE_LEN, blocks[0].end_hour + 1, 23);
                let min_price = (charge_block.mean_price / INVERTER_EFFICIENCY).max(MIN_USE_TARIFF);
                blocks = Self::get_use_block(&tariffs, min_price, USE_LEN, charge_block.end_hour + 1, 23);
                if !blocks.is_empty() {
                    schedule.blocks.push(charge_block);
                    schedule.blocks.push(blocks[0].clone());
                }
            // If there are more than one use block found we try to squeeze a charge block in between,
            // and if that is not possible we instead choose the best of the two first use blocks
            } else if blocks.len() >= 1 {
                let charge_block = Self::get_charge_block(&tariffs, CHARGE_LEN, blocks[0].end_hour + 1, blocks[1].start_hour - 1);
                let min_price = (charge_block.mean_price / INVERTER_EFFICIENCY).max(MIN_USE_TARIFF);
                let new_blocks = Self::get_use_block(&tariffs, min_price, USE_LEN, charge_block.end_hour + 1, 23);
                if !new_blocks.is_empty() {
                    schedule.blocks.push(charge_block);
                    schedule.blocks.push(new_blocks[0].clone());
                } else if blocks[1].mean_price > blocks[0].mean_price {
                    schedule.blocks.pop();
                    schedule.blocks.push(blocks[1].clone());
                }
                blocks = new_blocks;
            }
        }

        Ok(Self::add_hold_blocks(schedule, tariffs).tariffs_to_array(tariffs)?)
    }

    /// Updates the schedule with charge levels for the charge blocks, i.e. what
    /// maxSoC should be given the estimated production and consumption following the charge
    ///
    /// The logic also ensures that a previous maxSoC can't be higher than a following since
    /// that would in theory mean that there will not be enough room for PV power later
    /// in the day (the battery SoC is potentially already over the following hours setting).
    ///
    /// # Arguments
    ///
    /// * 'production' - estimated production in watts per hour
    /// * 'consumption' - estimated consumption in watts per hour
    /// * 'is_update' - whether this is an update to a new schedule or not
    pub fn update_charge_levels(&mut self, production: &PVProduction, consumption: &Consumption, is_update: bool) {
        let mut min_max_soc: u8 = 100;
        for b in (0..(self.blocks.len() - 1)).rev() {
            if self.blocks[b].block_type == BlockType::Charge {
                let mut selected_hours: Vec<u8> = Vec::new();
                for b2 in (b + 1)..self.blocks.len() {
                    if self.blocks[b2].block_type == BlockType::Charge {
                        break;
                    }
                    for h in self.blocks[b2].start_hour..=self.blocks[b2].end_hour {
                        selected_hours.push(h);
                    }
                }
                let charge_level = Self::get_charge_level(selected_hours, &production, &consumption);
                min_max_soc = min_max_soc.min(charge_level);
                if self.blocks[b].max_soc != min_max_soc && is_update {
                    self.blocks[b].is_updated = true;
                }
                self.blocks[b].max_soc = min_max_soc;
            }
        }
        let day_charge_level = Self::get_charge_level((0..24).collect::<Vec<u8>>(), &production, &consumption);
        self.update_max_min_soc(day_charge_level, is_update);
    }

    /// Updates max minSoC for all non-charge blocks
    /// This to avoid setting a too high min soc on grid for a hold block if a previous
    /// block has been pushed higher than charge max soc (which reflects the wiggle room
    /// for PV power given production/consumption estimates)
    ///
    /// # Arguments
    ///
    /// * 'day_charge_level' - the charge level for the entire day
    /// * 'is_update' - whether this is an update to a new schedule or not
    fn update_max_min_soc(&mut self, day_charge_level: u8, is_update: bool) {
        if self.blocks.len() == 1 {
            if self.blocks[0].max_min_soc != day_charge_level && is_update {
                self.blocks[0].is_updated = true;
            }
            self.blocks[0].max_min_soc = day_charge_level;
        } else {
            let mut last_max_min_soc: u8 = 100;
            for b in 0..self.blocks.len() {
                if self.blocks[b].block_type == BlockType::Charge {
                    last_max_min_soc = self.blocks[b].max_soc;
                } else {
                    if self.blocks[b].max_min_soc != last_max_min_soc && is_update {
                        self.blocks[b].is_updated = true;
                    }
                    self.blocks[b].max_min_soc = last_max_min_soc;
                }
            }
        }
    }

    /// Updates block status for those blocks still in waiting but passed time
    ///
    pub fn update_status(mut self) -> Self {
        let local_now = Local::now();

        if local_now.timestamp() >= self.date.timestamp() {
            let now = local_now.hour() as u8;

            for s in self.blocks.iter_mut() {
                if s.end_hour < now && s.status == Status::Waiting {
                    s.status = Status::Missed;
                }
            }
        }
        self
    }

    /// Returns the index of the next block that is ready to start, or None if none found
    ///
    /// # Arguments
    ///
    /// * 'hour' - the hour to check for
    pub fn get_eligible_for_start(&self, hour: u8) -> Option<usize> {
        for (i, b) in self.blocks.iter().enumerate() {
            if b.status == Status::Waiting && b.start_hour <= hour && b.end_hour >= hour {
                return Some(i);
            }
        }

        None
    }

    /// Returns, if any, the index of a running charge block with status Started
    ///
    /// # Arguments
    ///
    /// * 'hour' - the hour to check for
    pub fn get_current_started_charge(&self, hour: u8) -> Option<usize> {
        for (i, _) in self.blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                b.block_type == BlockType::Charge && b.status == Status::Started && b.start_hour <= hour && b.end_hour >= hour
            }) {
            return Some(i);
        }

        None
    }

    /// Returns block index if the block meets stated conditions
    ///
    /// # Arguments
    ///
    /// * 'hour' - the hour to check for
    /// * 'block_type' - a list of block types
    /// * 'conditions' - a list of BlockType Status tuples
    pub fn get_conditional(&self, hour: u8, conditions: Vec<(&BlockType, &Status)>) -> Option<usize> {
        for (i, _) in self.blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                b.start_hour <= hour && b.end_hour >= hour && conditions.contains(&(&b.block_type, &b.status))
                //block_type.contains(&b.block_type) && status.contains(&b.status) && b.start_hour <= hour && b.end_hour >= hour
            }) {
            return Some(i);
        }

        None
    }

    /// Returns a clone of a block identified by its index
    ///
    /// # Arguments
    ///
    /// * 'block_idx' - index of the block to return
    pub fn get_block_clone(&self, block_idx: usize) -> Option<Block> {
        if block_idx < self.blocks.len() {
            Some(self.blocks[block_idx].clone())
        } else {
            None
        }
    }

    /// Updates a block identified by its index with a new status
    ///
    /// # Arguments
    ///
    /// * 'block_idx' - index of block to update
    /// * 'status' - the status to update with
    pub fn update_block_status(&mut self, block_idx: usize, status: Status) -> Result<(), SchedulingError>{
        if block_idx < self.blocks.len() {
            self.blocks[block_idx].status = status;
            Ok(())
        } else {
            Err(SchedulingError(format!("block index {} not found", block_idx)))
        }
    }

    /// Resets the is updated flag for a block identified by its index
    ///
    /// # Arguments
    ///
    /// * 'block_idx' - index of block to reset
    pub fn reset_is_updated(&mut self, block_idx: usize) {
        self.blocks[block_idx].is_updated = false;
    }

    /// Get charge level for a given day and selected hours.
    ///
    /// # Arguments
    ///
    /// * 'selected_hours' - hours of the date to include in calculation
    /// * 'production' - struct containing estimated hourly production levels
    /// * 'consumption' - struct containing estimated hourly load levels
    fn get_charge_level(selected_hours: Vec<u8>, production: &PVProduction, consumption: &Consumption) -> u8 {
        let segment = production.get_production()
            .iter().enumerate()
            .filter(|(h, _)| selected_hours.contains(&(*h as u8)))
            .map(|(h, &p)|
                 p - consumption.get_hour_consumption(h)
            )
            .filter(|&sc| sc > 0.0)
            .fold((0usize, 0.0f64), |acc, el| (acc.0 + 1, acc.1 + el));

        let charge_level: u8;
        if segment.0 == 0 {
            charge_level = 100;
        } else {
            charge_level = (100.0 - segment.1 / SOC_CAPACITY_W).floor() as u8;
        }

        if charge_level != 100 {
            charge_level.min(95)
        } else {
            100
        }
    }

    /// Adds hold blocks where there are no charge- or use blocks. Hold blocks tells the inverter to
    /// hold minimum charge att whatever SoC the previous block left with.
    ///
    /// # Arguments
    ///
    /// * 'schedule' - the schedule to fill hold blocks to
    /// * 'tariffs' - used to fill in mean price also for hold blocks
    fn add_hold_blocks(schedule: Schedule, tariffs: &Vec<f64>) -> Schedule {
        let mut new_schedule = Schedule { date: schedule.date, blocks: Vec::new(), tariffs: schedule.tariffs };
        if schedule.blocks.is_empty() {
            new_schedule.blocks.push(Self::create_block(tariffs, 0, 23, BlockType::Use));
            return new_schedule;
        }

        let mut next_start_hour: u8 = 0;
        for block in schedule.blocks {
            if block.start_hour != next_start_hour {
                new_schedule.blocks.push(Self::create_block(tariffs, next_start_hour, block.start_hour - 1, BlockType::Hold));
            }

            next_start_hour = block.end_hour + 1;
            new_schedule.blocks.push(block);
        }

        if next_start_hour != 24 {
            new_schedule.blocks.push(Self::create_block(tariffs, next_start_hour, 23, BlockType::Hold));
        }
        new_schedule
    }

    /// Helper function to avoid having clutter higher level functions
    ///
    /// It creates a new Block, fills in some default values and calculates mean price
    /// for the block
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - used to fill in mean price
    /// * 'start' - start hour for the block
    /// * 'end' - end hour (inclusive) for the block
    fn create_block(tariffs: &Vec<f64>, start: u8, end: u8, block_type: BlockType) -> Block {
        let hour_price = tariffs[start as usize..=end as usize].to_vec();
        Block {
            block_type,
            max_min_soc: 100,
            max_soc: 100,
            start_hour: start,
            end_hour: end,
            mean_price: hour_price.iter().sum::<f64>() / hour_price.len() as f64,
            hour_price,
            is_updated: false,
            status: Status::Waiting,
        }
    }

    /// Returns the best block with respect to the lowest mean price within the range start to end
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - NordPool tariffs
    /// * 'block_len' - length of charge block to return
    /// * 'start' - start hour to search within
    /// * 'end' - end hour (inclusive) to search within
    fn get_charge_block(tariffs: &Vec<f64>, block_len: u8, start: u8, end: u8) -> Block {
        let mut block: Block = Block {
            block_type: BlockType::Charge,
            max_min_soc: 100,
            max_soc: 100,
            start_hour: 0,
            end_hour: 0,
            mean_price: 10000.0,
            hour_price: Vec::new(),
            is_updated: false,
            status: Status::Waiting,
        };

        for hour in start..=end.min(24 - block_len) {
            let hour_price: Vec<f64> = tariffs[hour as usize..(hour + block_len) as usize].to_vec();
            let s = hour_price.iter().sum::<f64>() / block_len as f64;
            if s < block.mean_price {
                block.start_hour = hour;
                block.end_hour = hour + block_len - 1;
                block.mean_price = s;
                block.hour_price = hour_price;
            }
        }

        block
    }

    /// Returns all valid use block containing only tariffs on or higher than min price.
    /// The logic also sorts out any sub sets and intersecting sets, keeping the set with the highest
    /// mean price. Also, adjacent sets are sorted out since there is then no time to recharge
    /// batteries in between.
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - NordPool tariffs
    /// * 'min_price' - minimum price an hour must cost for it to be part of a use block
    /// * 'max_block_len' - maximum number of hours a use block is allowed to contain
    /// * 'start' - start hour to search within
    /// * 'end' - end hour (inclusive) to search within
    fn get_use_block(tariffs: &Vec<f64>, min_price: f64, max_block_len: u8, start: u8, end: u8) -> Vec<Block> {
        let mut blocks: Vec<Block> = Vec::new();

        for start_hour in start..=end {
            if tariffs[start_hour as usize] >= min_price {
                let mut prices: Vec<f64> = vec![tariffs[start_hour as usize]];
                for hour2 in (start_hour + 1)..(start_hour + max_block_len).min(24) {
                    if tariffs[hour2 as usize] >= min_price {
                        prices.push(tariffs[hour2 as usize]);
                    } else {
                        break;
                    }
                }
                blocks.push(Block {
                    block_type: BlockType::Use,
                    max_min_soc: 100,
                    max_soc: 100,
                    start_hour,
                    end_hour: start_hour + prices.len() as u8 - 1,
                    mean_price: prices.iter().sum::<f64>() / prices.len() as f64,
                    hour_price: prices,
                    is_updated: false,
                    status: Status::Waiting,
                });
            }
        }

        Self::filter_out_subsets(blocks)
    }

    /// Filters out subsets and adjacent sets, while keeping best set or sets in terms of
    /// high hour prices.
    ///
    /// # Arguments
    ///
    /// * 'blocks' - blocks to filter
    fn filter_out_subsets(blocks: Vec<Block>) -> Vec<Block> {
        let mut intermediate_blocks: Vec<Block> = Vec::new();
        let mut filtered_blocks: Vec<Block> = Vec::new();

        let mut set: HashSet<u8> = HashSet::new();
        let mut mean_price: f64 = 0.0;

        // Filter out succeeding blocks that are either subsets of a preceding block or
        // are intersects but with a lower mean price. Also, a succeeding block that is
        // an intersect, but with higher mean price, will replace the preceding block.
        for block in blocks {
            let next_set: HashSet<u8> = (block.start_hour..=block.end_hour).collect::<HashSet<u8>>();
            if !next_set.is_subset(&set) {
                if !next_set.is_disjoint(&set) {
                    if block.mean_price > mean_price {
                        intermediate_blocks.remove(intermediate_blocks.len() - 1);
                        set = next_set;
                        mean_price = block.mean_price;
                        intermediate_blocks.push(block);
                    }
                } else {
                    set = next_set;
                    mean_price = block.mean_price;
                    intermediate_blocks.push(block);
                }
            }
        }

        // Filter out any trailing adjacent block since there is then no possibility
        // to charge batteries in between
        let mut end_hour: u8 = 24;
        for block in intermediate_blocks {
            if block.start_hour != end_hour + 1 {
                end_hour = block.end_hour;
                filtered_blocks.push(block);
            }
        }

        filtered_blocks
    }

    /// Collects tariffs from vector nad stores values in Self
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - NordPool tariffs
    fn tariffs_to_array(&mut self, tariffs: &Vec<f64>) -> Result<Self, SchedulingError> {
        if tariffs.len() == 24 {
            tariffs.iter().enumerate().for_each(|(i, &t)| self.tariffs[i] = t);
            Ok(self.to_owned())
        } else {
            Err(SchedulingError(format!("Tariffs vector illegal length: {}", tariffs.len())))
        }
    }
}

/// Creates a new schedule including updating charge levels
///
/// # Arguments
///
/// * 'nordpool' - reference to a NordPool struct
/// * 'smhi' - reference to a SMHI struct
/// * 'date_time' - the date for which the schedule shall be created
/// * 'backup_dir' - the path to the backup directory
pub fn create_new_schedule(nordpool: &NordPool, smhi: &mut SMHI, date_time: DateTime<Local>, backup_dir: &str) -> Result<Schedule, SchedulingError> {
    let forecast = retry!(||smhi.new_forecast(date_time))?;
    let production = PVProduction::new(&forecast, LAT, LONG);
    let consumption = Consumption::new(&forecast);
    let tariffs = retry!(||nordpool.get_tariffs(date_time))?;
    let mut schedule = Schedule::from_tariffs(&tariffs, date_time)?.update_status();
    schedule.update_charge_levels(&production, &consumption, false);
    save_backup(backup_dir, date_time, forecast, production.get_production(), consumption.get_consumption(), &schedule)?;

    Ok(schedule)
}

/// Updates an existing schedule with updated charge levels
///
/// # Arguments
///
/// * 'schedule' - a mutable reference to an existing schedule to be updated
/// * 'smhi' - reference to a SMHI struct
/// * 'backup_dir' - the path to the backup directory
pub fn update_existing_schedule(schedule: &mut Schedule, smhi: &mut SMHI, backup_dir: &str) -> Result<(), SchedulingError> {
    let local_now = Local::now();
    let forecast = retry!(||smhi.new_forecast(local_now))?;
    let production = PVProduction::new(&forecast, LAT, LONG);
    let consumption = Consumption::new(&forecast);
    schedule.update_charge_levels(&production, &consumption, true);
    save_backup(backup_dir, local_now, forecast, production.get_production(), consumption.get_consumption(), schedule)?;

    Ok(())
}

/// Saves a backup of the current schedule including current charge levels
/// This is very similar to the function update_existing_schedule, but it doesn't fetch a new
/// forecast from SMHI, rather it uses the last one fetched.
///
/// # Arguments
///
/// * 'schedule' - a mutable reference to an existing schedule to be updated
/// * 'smhi' - reference to a SMHI struct
/// * 'backup_dir' - the path to the backup directory
pub fn backup_schedule(schedule: &Schedule, smhi: &SMHI, backup_dir: &str) -> Result<(), SchedulingError> {
    let local_now = Local::now();
    let forecast = smhi.get_forecast().clone();
    let production = PVProduction::new(&forecast, LAT, LONG);
    let consumption = Consumption::new(&forecast);
    save_backup(backup_dir, local_now, forecast, production.get_production(), consumption.get_consumption(), schedule)?;

    Ok(())
}
