use std::collections::{HashSet};
use std::{fmt, fs};
use std::fs::File;
use std::io::Read;
use std::ops::Add;
use std::thread;
use std::time::Duration;
use chrono::{DateTime, Local, TimeDelta, Timelike};
use glob::glob;
use serde::{Deserialize, Serialize};
use crate::consumption::Consumption;
use crate::manager_nordpool::NordPool;
use crate::manager_smhi::SMHI;
use crate::production::PVProduction;
use crate::{retry, wrapper, LAT, LONG};

/// Time needed to fully charge batteries from SoC 10% to SoC 100%
const CHARGE_LEN: u8 = 3;

/// Time available in batteries to supply power when in self use mode
const USE_LEN: u8 = 5;

/// The battery capacity in watts divided by the max SoC (State of Charge). This represents
/// roughly how much each percentage of the SoC is in terms of power (Wh)
const SOC_CAPACITY_W: f64 = 16590.0 / 100.0;

/// Available block types
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockType {
    Charge,
    Hold,
    Use,
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for BlockType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
    pub status: Status,
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} - {} -> {:>2} - {:>2}: maxMinSoc {:<3}, maxSoc {:<3}, price {:0.3} {:?}",
               self.status, self.block_type, self.start_hour, self.end_hour,
               self.max_min_soc, self.max_soc, self.mean_price, self.hour_price)
    }
}

/// Struct representing one day's block schedule
#[derive(Serialize, Deserialize)]
pub struct Schedule {
    pub date: DateTime<Local>,
    pub blocks: Vec<Block>,
}

impl Schedule {
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
    pub fn from_tariffs(tariffs: &Vec<f64>) -> Schedule {
        let mut schedule = Schedule { date: Local::now(), blocks: Vec::new() };
        let segments: [(u8,u8);3] = [(0,8 - CHARGE_LEN), (8, 16 - CHARGE_LEN), (16, 24 - CHARGE_LEN)];

        // Find the best charge block with following use block(s) where mean price for a use block is
        // at least 25% more expensive (to factor in inverter/battery efficiency factor of
        // roughly 80% efficiency full circle). The day is divided in three segments, but only
        // the charge block is affected by that boundary.
        let mut blocks: Vec<Block> = Vec::new();
        for s in segments.iter() {
            let charge_block = Self::get_charge_block(&tariffs, CHARGE_LEN, s.0, s.1);
            blocks = Self::get_use_block(&tariffs, charge_block.mean_price / 0.8, USE_LEN,
                                         charge_block.end_hour + 1, 23);
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
                blocks = Self::get_use_block(&tariffs, charge_block.mean_price / 0.8, USE_LEN, charge_block.end_hour + 1, 23);
                if !blocks.is_empty() {
                    schedule.blocks.push(charge_block);
                    schedule.blocks.push(blocks[0].clone());
                }
            // If there are more than one use block found we try to squeeze a charge block in between,
            // and if that is not possible we instead choose the best of the two first use blocks
            } else if blocks.len() >= 1 {
                let charge_block = Self::get_charge_block(&tariffs, CHARGE_LEN, blocks[0].end_hour + 1, blocks[1].start_hour - 1);
                let new_blocks = Self::get_use_block(&tariffs, charge_block.mean_price / 0.8, USE_LEN, charge_block.end_hour + 1, 23);
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

        Self::add_hold_blocks(schedule, tariffs)
    }

    /// Updates the schedule with charge levels for the charge blocks, i.e. what
    /// maxSoC should be given estimated production and consumption following the charge
    ///
    /// # Arguments
    ///
    /// * 'production' - estimated production in watts per hour
    /// * 'consumption' - estimated consumption in watts per hour
    pub fn update_charge_levels(&mut self, production: &PVProduction, consumption: &Consumption) {
        for b in 0..(self.blocks.len() - 1) {
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
                self.blocks[b].max_soc = charge_level;
            }
        }
        self.update_max_min_soc();
    }

    /// Updates max minSoC for all non-charge blocks
    /// This to avoid setting a too high min soc on grid for a hold block if a previous
    /// block has been pushed higher than charge max soc (which reflects the wiggle room
    /// for PV power given production/consumption estimates)
    ///
    fn update_max_min_soc(&mut self) {
        if self.blocks.len() == 1 {
            self.blocks[0].max_min_soc = 50;
        } else {
            let mut last_max_min_soc: u8 = 100;
            for b in 0..self.blocks.len() {
                if self.blocks[b].block_type == BlockType::Charge {
                    last_max_min_soc = self.blocks[b].max_soc;
                } else {
                    self.blocks[b].max_min_soc = last_max_min_soc;
                }
            }
        }
    }

    /// Updates block status for those blocks still in waiting but passed time
    ///
    pub fn update_status(mut self) -> Self {
        let now = Local::now().hour() as u8;
        for s in self.blocks.iter_mut() {
            if s.end_hour < now && s.status == Status::Waiting {
                s.status = Status::Missed;
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

    /// Returns, if any, the index of a currently running charge block with status Started
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
    pub fn update_block_status(&mut self, block_idx: usize, status: Status) -> Result<(), String>{
        if block_idx < self.blocks.len() {
            self.blocks[block_idx].status = status;
            Ok(())
        } else {
            Err(format!("block index {} not found", block_idx))
        }
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
                Self::calculate_spare_capacity(p, consumption.get_consumption(h), consumption.get_min_avg_load())
            )
            .filter(|sc| sc.round() as i64 != 0)
            .fold((0usize, 0.0f64), |acc, el| (acc.0 + 1, acc.1 + el));

        let charge_level: u8;
        if segment.0 == 0 {
            charge_level = 100;
        } else {
            charge_level = (100.0 - (segment.1 / segment.0 as f64) / SOC_CAPACITY_W).floor() as u8
        }

        if charge_level != 100 {
            charge_level.min(95)
        } else {
            100
        }
    }

    /// Calculates what spare capacity in watts that is needed to cover for either irregularities
    /// in the load when load is greater than production, or room needed when production is greater
    /// than load. If production is zero however we don't need any spare capacity at all.
    ///
    /// # Arguments
    ///
    /// * 'production' - production in watts from PV
    /// * 'load' - the household load in watts (i.e. not grid consumption which may include battery charging)
    /// * 'min_avg_load' - min average consumption/load in watts over an hour
    fn calculate_spare_capacity(production: f64, load: f64, min_avg_load: f64) -> f64 {
        let mut diff = production - load;
        if diff < 0.0 {
            if production.round() as i64 == 0 {
                diff = 0.0;
            } else {
                diff = ((production - min_avg_load).max(0.0) / 2.0).max(5.0 * SOC_CAPACITY_W);
            }
        } else {
            diff = diff.max(10.0 * SOC_CAPACITY_W);
        }
        diff
    }

    /// Adds hold blocks where there are no charge- or use blocks. Hold blocks tells the inverter to
    /// hold minimum charge att whatever SoC the previous block left with.
    ///
    /// # Arguments
    ///
    /// * 'schedule' - the schedule to fill hold blocks to
    /// * 'tariffs' - used to fill in mean price also for hold blocks
    fn add_hold_blocks(schedule: Schedule, tariffs: &Vec<f64>) -> Schedule {
        let mut new_schedule = Schedule { date: schedule.date, blocks: Vec::new() };
        if schedule.blocks.is_empty() {
            new_schedule.blocks.push(Self::create_hold_block(tariffs, 0, 23));
            return new_schedule;
        }

        let mut next_start_hour: u8 = 0;
        for block in schedule.blocks {
            if block.start_hour != next_start_hour {
                new_schedule.blocks.push(Self::create_hold_block(tariffs, next_start_hour, block.start_hour - 1));
            }

            next_start_hour = block.end_hour + 1;
            new_schedule.blocks.push(block);
        }

        if next_start_hour != 24 {
            new_schedule.blocks.push(Self::create_hold_block(tariffs, next_start_hour, 23));
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
    fn create_hold_block(tariffs: &Vec<f64>, start: u8, end: u8) -> Block {
        let hour_price = tariffs[start as usize..=end as usize].to_vec();
        Block {
            block_type: BlockType::Hold,
            max_min_soc: 100,
            max_soc: 100,
            start_hour: start,
            end_hour: end,
            mean_price: hour_price.iter().sum::<f64>() / hour_price.len() as f64,
            hour_price,
            status: Status::Waiting,
        }
    }

    /// Returns the best block with respect to lowest mean price within the range start to end
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
            max_soc: 0,
            start_hour: 0,
            end_hour: 0,
            mean_price: 10000.0,
            hour_price: Vec::new(),
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

}

/// Creates a new schedule including updating charge levels
///
/// # Arguments
///
/// * 'nordpool' - reference to a NordPool struct
/// * 'SMHI' - reference to a SMHI struct
/// * 'future' - optional future in days, i.e. if set to 1 it will create a schedule for tomorrow
pub fn create_new_schedule(nordpool: &NordPool, smhi: &SMHI, future: Option<usize>) -> Result<Schedule, String> {
    let mut d = 0;
    if let Some(f) = future { d = f as i64 }
    let forecast = retry!(||smhi.get_forecast(Local::now().add(TimeDelta::days(d))))?;
    let production = PVProduction::new(&forecast, LAT, LONG);
    let consumption = Consumption::new(&forecast);
    let tariffs = retry!(||nordpool.get_tariffs(Local::now().add(TimeDelta::days(d))))?;
    let mut schedule = Schedule::from_tariffs(&tariffs).update_status();
    schedule.update_charge_levels(&production, &consumption);

    Ok(schedule)
}

/// Updates an existing schedule with updated charge levels
///
/// # Arguments
///
/// * 'schedule' - a mutable reference to an existing schedule to be updated
/// * 'smhi' - reference to a SMHI struct
pub fn update_existing_schedule(schedule: &mut Schedule, smhi: &SMHI) {
    match retry!(||smhi.get_forecast(Local::now())) {
        Ok(forecast) => {
            let production = PVProduction::new(&forecast, LAT, LONG);
            let consumption = Consumption::new(&forecast);
            schedule.update_charge_levels(&production, &consumption);
        },
        Err(e) => {
            eprintln!("Error updating schedule for block: {}", e);
            eprintln!("This is recoverable, it only affects charge levels")
        }
    }
}

/// Saves schedule to file as json
/// The filename gives at most one unique file per hour
///
/// # Arguments
///
/// * 'schedule' - the schedule to save
/// * 'backup_dir' - the directory to save the file to
pub fn save_schedule(schedule: &Schedule, backup_dir: &str) {
    let err: String;
    let file_path = format!("{}{}.json", backup_dir, Local::now().format("%Y%m%d_%H"));
    match serde_json::to_string(&schedule) {
        Ok(json) => {
            match fs::write(file_path, json) {
                Ok(_) => { return },
                Err(e) => { err = e.to_string() }
            }
        },
        Err(e) => { err = e.to_string() }
    }
    eprintln!("Error writing schedule to file: {}", err);
    eprintln!("This is recoverable")
}

/// Loads schedule from json on file
///
/// This is mostly to avoid re-executing an already started block, hence it will just find
/// the latest saved schedule for the current day
///
/// # Arguments
///
/// * 'backup_dir' - the directory to save the file to
pub fn load_schedule(backup_dir: &str) -> Result<Option<Schedule>, String> {
    let mut entries: Vec<String> = Vec::new();
    let file_path = format!("{}{}.json", backup_dir, Local::now().format("%Y%m%d*"));
    for entry in glob(&file_path)
        .map_err(|e| format!("Error searching directory: {}", e.to_string()))? {
        match entry {
            Ok(path) => {
                if path.is_file() {
                    if let Some(os_path) = path.to_str() {
                        entries.push(os_path.to_string());
                    }
                }
            },
            Err(e) => {
                return Err(format!("Error reading directory entry: {}", e.to_string()));
            }
        }
    }

    entries.sort();

    if entries.len() > 0 {
        match File::open(&entries[entries.len() - 1]) {
            Ok(mut file) => {
                let mut contents = String::new();
                match file.read_to_string(&mut contents).map_err(|e| e.to_string()) {
                    Ok(_) => {
                        let schedule: Schedule = serde_json::from_str(&contents)
                            .map_err(|e| format!("Error while parsing json to Schedule: {}", e.to_string()))?;
                        Ok(Some(schedule))
                    },
                    Err(e) => { Err(format!("Error while reading backup file: {}", e.to_string())) }
                }
            },
            Err(e) => { Err(format!("Error while open schedule file: {}", e.to_string())) }
        }
    } else {
        Ok(None)
    }
}
