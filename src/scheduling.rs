use std::fmt;
use std::fmt::Formatter;
use std::thread;
use chrono::{DateTime, Local, NaiveDate, Timelike};
use serde::{Deserialize, Serialize};
use crate::consumption::Consumption;
use crate::production::PVProduction;

use std::collections::HashMap;
use crate::errors::SchedulingError;
use crate::manager_nordpool::NordPool;
use crate::manager_smhi::SMHI;
use crate::{retry, wrapper, LAT, LONG};
use crate::backup::save_base_data;

const BAT_CAPACITY: f64 = 16590.0 / 1000.0;
const BAT_KWH: f64 = BAT_CAPACITY * 0.9;
pub const SOC_KWH: f64 = BAT_CAPACITY / 100.0;
const CHARGE_KWH_HOUR: f64 = 6.0;
const CHARGE_EFFICIENCY: f64 = 0.9;
const DISCHARGE_EFFICIENCY: f64 = 0.9;
const SELL_PRIORITY: f64 = 0.0;

#[derive(Serialize, Deserialize, Clone)]
struct Tariffs {
    buy: [f64;24],
    sell: [f64;24],
}

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
    Full(usize),
    Error,
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for Status {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Status::Waiting => write!(f, "Waiting  "),
            Status::Started => write!(f, "Started  "),
            Status::Missed  => write!(f, "Missed   "),
            Status::Full(soc) => write!(f, "Full: {:>3}", soc),
            Status::Error   => write!(f, "Error    "),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Block {
    pub block_type: BlockType,
    pub date: NaiveDate,
    pub start_hour: usize,
    pub end_hour: usize,
    size: usize,
    tariffs: Option<Tariffs>,
    pub charge_tariff_in: f64,
    pub charge_tariff_out: f64,
    pub price: f64,
    pub charge_in: f64,
    pub charge_out: f64,
    pub overflow: f64,
    pub overflow_price: f64,
    pub soc_in: usize,
    pub soc_out: usize,
    pub status: Status,
}

/// Implementation of the Display Trait for pretty print
impl fmt::Display for Block {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {

        // Build base output
        let output = format!("{} - {} -> {:>2} - {:>2}: SocIn {:>3}, SocOut {:>3}, chargeIn {:>5.2}, chargeOut {:>5.2}, chargeTariffIn {:>5.2}, chargeTariffOut {:>5.2}, overflow {:>5.2}, overflowPrice {:>5.2}, price {:>5.2} ",
                                 self.status, self.block_type,
                                 self.start_hour, self.end_hour,
                                 self.soc_in, self.soc_out,
                                 self.charge_in, self.charge_out,
                                 self.charge_tariff_in, self.charge_tariff_out,
                                 self.overflow, self.overflow_price,
                                 self.price);

        write!(f, "{}", output)
    }
}

impl Block {
    /// Returns true if the block is active in relation to date and time
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the date time the block is valid for
    pub fn is_active(&self, date_time: DateTime<Local>) -> bool {
        let date = date_time.date_naive();
        let hour = date_time.hour() as usize;

        self.date == date && hour >= self.start_hour && hour <= self.end_hour
    }

    /// Returns true if the block is a started charge block and not prematurely stopped
    ///
    pub fn is_charge(&self) -> bool {
        self.block_type == BlockType::Charge && self.status == Status::Started
    }

    /// Updates a block with a new status
    /// If the block is a Charge block and the new status is Full then a new charge tariff out is
    /// calculated. Also, the soc out and charge out is updated to reflect the actual soc.
    ///
    /// # Arguments
    ///
    /// * 'status' - the status to update with
    pub fn update_block_status(&mut self, status: Status) {
        if self.block_type == BlockType::Charge {
            if let Status::Full(soc) = status {
                if soc > self.soc_in {
                    let tariffs = self.tariffs.as_ref().unwrap();
                    let charge_in_bat = (soc - self.soc_in) as f64 * SOC_KWH;
                    let (cost, _) = charge_cost_charge_end(self.start_hour, charge_in_bat / CHARGE_EFFICIENCY, tariffs);
                    let charge_tariff = cost / charge_in_bat;
                    self.charge_tariff_out = ((self.soc_in - 10) as f64 * self.charge_tariff_in + (soc - self.soc_in) as f64 * charge_tariff) / (soc - 10) as f64;
                }
                self.soc_out = soc;
                self.charge_out = (soc - 10) as f64 * SOC_KWH;
            }
        }
        self.status = status;
    }
}

#[derive(Clone)]
struct Blocks {
    schedule_id: u32,
    blocks: Vec<Block>,
    next_start: usize,
    next_charge_in: f64,
    next_charge_tariff_in: f64,
    next_soc_in: usize,
    total_price: f64,
}

/// Struct representing one day's block schedule
#[derive(Serialize, Deserialize, Clone)]
pub struct Schedule {
    pub date: DateTime<Local>,
    pub blocks: Vec<Block>,
    tariffs: Tariffs,
}

impl Schedule {
    /// Creates a new schedule based on tariffs, production and consumption estimates.
    /// It can also base the schedule on any residual charge and its mean charge tariff carrying in
    /// from a previous schedule or from the inverter itself (in which case it might be hard to determine
    /// the mean charge tariff).
    ///
    /// # Arguments
    ///
    /// * 'start' - start hour for the schedule to be created
    /// * 'tariffs' - tariffs as given from NordPool
    /// * 'production' - production estimates per hour
    /// * 'consumption' - consumption estimates per hour
    /// * 'charge_in' - any residual charge to bear in to the new schedule
    /// * 'charge_tariff_in' - the mean price for the residual price
    /// * 'date_time' - the date time to stamp on the schedule
    pub fn new_with_scheduling(start: usize, tariffs: &Vec<f64>, production: &PVProduction, consumption: &Consumption, charge_in: f64, charge_tariff_in: f64, date_time: DateTime<Local>) -> Schedule {
        let prod = production.get_production();
        let cons = consumption.get_consumption();
        let mut net_prod: [f64;24] = [0.0;24];
        prod.iter()
            .enumerate()
            .for_each(|(i, &p)| net_prod[i] = (p - cons[i]) / 1000.0);

        let tariffs = split_tariffs(&tariffs);

        let blocks = seek_best(start, &tariffs, net_prod, charge_in, charge_tariff_in, date_time);

        Schedule {
            date: date_time,
            blocks: blocks.blocks,
            tariffs,
        }
    }

    /// Updates a block identified by its running hours
    ///
    /// # Arguments
    ///
    /// * 'hour' - hour to identify block with
    /// * 'status' - the status to update with
    pub fn update_block_status(&mut self, hour: usize, status: Status) {
        for b in self.blocks.iter_mut() {
            if b.status == Status::Waiting && b.start_hour <= hour && b.end_hour >= hour {
                b.status = status;
                return;
            }
        }
    }

    /// Returns a clone of a block identified by hour
    ///
    /// # Arguments
    ///
    /// * 'hour' - the hour to get a block for
    pub fn get_block(&self, hour: usize) -> Result<Block, SchedulingError> {
        for b in self.blocks.iter() {
            if b.start_hour <= hour && b.end_hour >= hour {
                return Ok(b.clone());
            }
        }

        Err(SchedulingError(format!("no block in schedule corresponding to hour {}", hour)))
    }
}

/// Seeks the best schedule given input parameters.
/// The algorithm searches through all combinations of charge blocks, use blocks and charge levels
/// and returns the one with the best price (i.e. the mean price for usage minus the price for charging).
/// It also considers charge input from PV, which not only tops up batteries but also lowers the
/// mean price for the stored energy, which in turn can be used for even lower hourly tariffs.
///
/// # Arguments
///
/// * 'start' - start hour for the schedule to be created
/// * 'tariffs' - tariffs as given from NordPool but marked up with VAT, fees and other price components
/// * 'net_prod' - the net production (production minus consumption) per hour
/// * 'charge_in' - any residual charge to bear in to the new schedule
/// * 'charge_tariff_in' - the mean price for the residual price
/// * 'date_time' - the date to stamp on the block
fn seek_best(start: usize, tariffs: &Tariffs, net_prod: [f64;24], charge_in: f64, charge_tariff_in: f64, date_time: DateTime<Local>) -> Blocks {
    let mut schedule_id: u32 = 0;
    let mut record: HashMap<usize, Blocks> = create_base_blocks(schedule_id, charge_in, charge_tariff_in, &tariffs, net_prod, date_time);

    for seek_first_charge in start..23 {
        for charge_level_first in 0..=90 {
            schedule_id += 1;

            let first_charge_blocks = seek_charge(start, seek_first_charge, charge_level_first, &tariffs, net_prod, charge_in, charge_tariff_in, date_time);
            for seek_first_use in first_charge_blocks.next_start..24 {

                if let Some(first_use_blocks) = seek_use(schedule_id, first_charge_blocks.next_start, seek_first_use, &tariffs, net_prod, first_charge_blocks.next_charge_in, first_charge_blocks.next_charge_tariff_in, date_time) {
                    let first_combined = combine_blocks(&first_charge_blocks, &first_use_blocks);
                    record_best(1, &first_combined, &tariffs, net_prod, &mut record, date_time);

                    for seek_second_charge in first_combined.next_start..23 {
                        for charge_level_second in 0..=90 {
                            schedule_id += 1;

                            let second_charge_blocks = seek_charge(first_combined.next_start, seek_second_charge, charge_level_second, &tariffs, net_prod, first_combined.next_charge_in, first_combined.next_charge_tariff_in, date_time);
                            for seek_second_use in second_charge_blocks.next_start..24 {

                                if let Some(second_use_blocks) = seek_use(schedule_id, second_charge_blocks.next_start, seek_second_use, &tariffs, net_prod, second_charge_blocks.next_charge_in, second_charge_blocks.next_charge_tariff_in, date_time) {
                                    let second_combined = combine_blocks(&second_charge_blocks, &second_use_blocks);
                                    let all_combined = combine_blocks(&first_combined, &second_combined);
                                    record_best(2, &all_combined, &tariffs, net_prod, &mut record, date_time);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    get_best(&record)
}

/// Returns the best block from what has been recorded for various levels
///
/// # Arguments
///
/// * 'record' - the record of the best Blocks structs saved so far
fn get_best(record: &HashMap<usize, Blocks>) -> Blocks {
    let mut best_total: f64 = -10000.0;
    let mut best_level: usize = 0;
    for l in record.keys() {
        if record[l].total_price > best_total {
            best_total = record[l].total_price;
            best_level = *l;
        }
    }

    let mut best_block = record[&best_level].clone();
    for ib in best_block.blocks.iter_mut() {
        ib.end_hour = ib.start_hour + ib.size - 1;
    }

    best_block
}

/// Creates an initial base block as a backstop if the search doesn't find any charge/use
/// opportunities.
///
/// # Arguments
///
/// * 'schedule_id' - an id that can be later used if some debugging is needed
/// * 'charge_in' - residual charge from previous block (or from previous day)
/// * 'charge_tariff_in' - mean tariff for residual charge
/// * 'tariffs' - tariffs (both buy and sell tariffs will be used) inc VAT and extra
/// * 'net_prod' - the net production (production minus consumption) per hour
/// * 'date_time' - the date to stamp on the block
fn create_base_blocks(schedule_id: u32, charge_in: f64, charge_tariff_in: f64, tariffs: &Tariffs, net_prod: [f64;24], date_time: DateTime<Local>) -> HashMap<usize, Blocks> {
    let mut record: HashMap<usize, Blocks> = HashMap::new();

    let (charge_out, charge_tariff_out, overflow, _) = update_for_pv(BlockType::Use, 0, 24, tariffs, net_prod, charge_in, charge_tariff_in);

    let block = Block {
        block_type: BlockType::Use,
        date: date_time.date_naive(),
        start_hour: 0,
        end_hour: 23,
        size: 24,
        tariffs: None,
        charge_tariff_in,
        charge_tariff_out,
        price: 0.0,
        charge_in,
        charge_out,
        overflow,
        overflow_price: 0.0,
        soc_in: 10 + (charge_in / SOC_KWH).round().min(90.0) as usize,
        soc_out: 10 + (charge_out / SOC_KWH).round().min(90.0) as usize,
        status: Status::Waiting,
    };

    record.insert(0,Blocks {
        schedule_id,
        next_start: 24,
        next_charge_in: block.charge_out,
        next_charge_tariff_in: block.charge_tariff_out,
        next_soc_in: block.soc_out,
        total_price: block.price,
        blocks: vec![block],
    });

    record
}

/// Trims out any blocks with zero size (they are just artifacts from the search flow).
/// Also, it makes sure that we fill any empty tail with a suitable hold block
///
/// # Arguments
///
/// * 'blocks' - the Blocks struct to trim and add tail to
/// * 'tariffs' - tariffs inc VAT and extra
/// * 'net_prod' - the net production (production minus consumption) per hour
/// * 'date_time' - the date to stamp on the block
fn trim_and_tail(blocks: &Blocks, tariffs: &Tariffs, net_prod: [f64;24], date_time: DateTime<Local>) -> Blocks {
    let mut result = blocks.clone();

    // Trim blocks with no length
    result.blocks = result.blocks.iter().filter(|b| b.size > 0).cloned().collect::<Vec<Block>>();

    let (charge_out, charge_tariff_out, overflow, _) = update_for_pv(BlockType::Hold, result.next_start, 24, tariffs, net_prod, result.next_charge_in, result.next_charge_tariff_in);

    if result.next_start < 24 {
        result.blocks.push({
            Block {
                block_type: BlockType::Hold,
                date: date_time.date_naive(),
                start_hour: result.next_start,
                end_hour: 0,
                size: 24 - result.next_start,
                tariffs: None,
                charge_tariff_in: result.next_charge_tariff_in,
                charge_tariff_out,
                price: 0.0,
                charge_in: result.next_charge_in,
                charge_out,
                overflow,
                overflow_price: 0.0,
                soc_in: result.next_soc_in,
                soc_out: 10 + (charge_out / SOC_KWH).round().min(90.0) as usize,
                status: Status::Waiting,
            }
        });
        result.next_start = 24;
        result.next_charge_in = charge_out;
        result.next_charge_tariff_in = charge_tariff_out;
        result.next_soc_in = 10 + (charge_out / SOC_KWH).round().min(90.0) as usize;
    }
    result
}

/// Combines two Blocks struct into one to get a complete day schedule
///
/// # Arguments
///
/// * 'blocks_one' - a blocks struct from a level one search
/// * 'blocks_two' - a blocks struct from a subsequent level two search
fn combine_blocks(blocks_one: &Blocks, blocks_two: &Blocks) -> Blocks {
    let mut combined = Blocks {
        schedule_id: blocks_two.schedule_id,
        blocks: blocks_one.blocks.clone(),
        next_start: blocks_two.next_start,
        next_charge_in: blocks_two.next_charge_in,
        next_charge_tariff_in: blocks_two.next_charge_tariff_in,
        next_soc_in: blocks_two.next_soc_in,
        total_price: blocks_one.total_price + blocks_two.total_price,
    };
    combined.blocks.extend(blocks_two.blocks.clone());

    combined
}

/// Saves the given Blocks struct if the total price is better than any stored for the level
///
/// # Arguments
///
/// * 'level' - level is 1 or 2 and indicates whether it is a first search result or a combined 2 step search
/// * 'blocks' - the Blocks struct to check and potentially save as the best for its level
/// * 'tariffs' - tariffs inc VAT and extra
/// * 'net_prod' - the net production (production minus consumption) per hour
/// * 'record' - the record of the best Blocks structs saved so far
/// * 'date_time' - the date to stamp on the block
fn record_best(level: usize, blocks: &Blocks, tariffs: &Tariffs, net_prod: [f64;24], record: &mut HashMap<usize, Blocks>, date_time: DateTime<Local>) {
    if let Some(recorded_blocks) = record.get(&level) {
        if blocks.total_price > recorded_blocks.total_price {
            record.insert(level, trim_and_tail(blocks, tariffs, net_prod, date_time));
        }
    } else {
        record.insert(level, trim_and_tail(blocks, tariffs, net_prod, date_time));
    }
}

/// Seeks a use block
///
/// # Arguments
///
/// * 'schedule_id' - an id that can be later used if some debugging is needed
/// * 'initial_start' - the initial start is used to calculate for a hold block prepending the charge block to bee
/// * 'seek_start' - where this run is supposed to start its search
/// * 'tariffs' - tariffs (both buy and sell tariffs will be used) inc VAT and extra
/// * 'net_prod' - the net production (production minus consumption) per hour
/// * 'charge_in' - residual charge from previous block
/// * 'charge_tariff_in' - mean tariff for residual charge
/// * 'date_time' - the date to stamp on the block
fn seek_use(schedule_id: u32, initial_start: usize, seek_start: usize, tariffs: &Tariffs, net_prod: [f64;24], charge_in: f64, charge_tariff_in: f64, date_time: DateTime<Local>) -> Option<Blocks> {

    for u_start in seek_start..24 {
        // for the hold phase
        let (hold_charge_out, hold_charge_tariff_out, hold_overflow, hold_overflow_price) = update_for_pv(BlockType::Hold, initial_start, u_start, tariffs, net_prod, charge_in, charge_tariff_in);
        let hold = Block {
            block_type: BlockType::Hold,
            date: date_time.date_naive(),
            start_hour: initial_start,
            end_hour: 0,
            size: u_start - initial_start,
            tariffs: None,
            charge_tariff_in,
            charge_tariff_out: hold_charge_tariff_out,
            price: 0.0,
            charge_in,
            charge_out: hold_charge_out,
            overflow: hold_overflow,
            overflow_price: hold_overflow_price,
            soc_in: 10 + (charge_in / SOC_KWH).round().min(90.0) as usize,
            soc_out: 10 + (hold_charge_out / SOC_KWH).round().min(90.0) as usize,
            status: Status::Waiting,
        };
        let mut charge_out:f64 = hold_charge_out;
        let mut charge_tariff_out:f64 = hold_charge_tariff_out;
        let mut overflow:f64 = hold_overflow;
        let mut overflow_price:f64 = hold_overflow_price;
        for u_end in u_start..=24 {
            if u_end > 23 || tariffs.buy[u_end] <= charge_tariff_out {
                if u_end != u_start {
                    return Some(get_use_blocks(schedule_id, u_start, u_end, charge_out, charge_tariff_out, overflow, overflow_price, hold, tariffs, net_prod, date_time));
                }
                break;
            }

            (charge_out, charge_tariff_out, overflow, overflow_price) = update_for_pv(BlockType::Use, u_start, u_end+1, tariffs, net_prod, hold_charge_out, hold_charge_tariff_out);

            if charge_out.round() == 0.0 {
                if u_end != u_start {
                    return Some(get_use_blocks(schedule_id, u_start, u_end + 1, charge_out, charge_tariff_out, overflow, overflow_price, hold, tariffs, net_prod, date_time));
                }
                break;
            }
        }
    }

    None
}

/// Creates a charge block
///
/// # Arguments
///
/// * 'start' - the charge block starting hour
/// * 'size' - length of charge block
/// * 'tariffs' - tariffs from NordPool
/// * 'charge_in' - residual charge from previous block
/// * 'charge_tariff_in' - mean tariff for residual charge
/// * 'charge_out' - charge out after charging
/// * 'charge_tariff_out' - mean tariff for charge in charge block
/// * 'price' - the price, or cost, for charging
/// * 'date_time' - the date to stamp on the block
fn get_charge_block(start: usize, size: usize, tariffs: &Tariffs, charge_in: f64, charge_tariff_in: f64, charge_out: f64, charge_tariff_out: f64, price: f64, date_time: DateTime<Local>) -> Block {
    Block {
        block_type: BlockType::Charge,
        date: date_time.date_naive(),
        start_hour: start,
        end_hour: 0,
        size,
        tariffs: Some(tariffs.clone()),
        charge_tariff_in,
        charge_tariff_out,
        price,
        charge_in,
        charge_out,
        overflow: 0.0,
        overflow_price: 0.0,
        soc_in: 10 + (charge_in / SOC_KWH).round().min(90.0) as usize,
        soc_out: 10 + (charge_out / SOC_KWH).round().min(90.0) as usize,
        status: Status::Waiting,
    }
}

/// Creates a use Blocks struct
///
/// # Arguments
///
/// * 'schedule_id' - an id that can be later used if some debugging is needed
/// * 'u_start' - the use block starting hour
/// * 'e_end' - the use block end hour (non-inclusive)
/// * 'charge_out' - residual charge from use block to create
/// * 'charge_tariff_out' - mean tariff for residual charge from the use block
/// * 'overflow' - overflow from the use block to create
/// * 'overflow_price' - price for the overflow from the use block
/// * 'hold' - the hold block between the charge and use block
/// * 'tariffs' - tariffs (buy tariffs used here) inc VAT and extra
/// * 'net_prod' - the net production (production minus consumption) per hour
/// * 'date_time' - the date to stamp on the block
fn get_use_blocks(schedule_id: u32, u_start: usize, u_end: usize, charge_out: f64, charge_tariff_out: f64, overflow: f64, overflow_price: f64, hold: Block, tariffs: &Tariffs, net_prod: [f64;24], date_time: DateTime<Local>) -> Blocks {
    let u_price = net_prod[u_start..u_end].iter()
        .zip(tariffs.buy[u_start..u_end].iter())
        .map(|(&np, &t)| np.min(0.0).abs()*t).sum::<f64>();

    let usage = Block {
        block_type: BlockType::Use,
        date: date_time.date_naive(),
        start_hour: u_start,
        end_hour: 0,
        size: u_end - u_start,
        tariffs: None,
        charge_tariff_in: hold.charge_tariff_out,
        charge_tariff_out,
        price: u_price,
        charge_in: hold.charge_out,
        charge_out,
        overflow,
        overflow_price,
        soc_in: hold.soc_out,
        soc_out: 10 + (charge_out / SOC_KWH).round().min(90.0) as usize,
        status: Status::Waiting,
    };

    Blocks {
        schedule_id,
        next_start: usage.start_hour + usage.size,
        next_charge_in: usage.charge_out,
        next_charge_tariff_in: usage.charge_tariff_out,
        next_soc_in: usage.soc_out,
        total_price: hold.price + hold.overflow_price + usage.price + usage.overflow_price,
        blocks: vec![hold, usage],
    }
}

/// Gets charge (and leading hold) block
///
/// # Arguments
///
/// * 'initial_start' - the initial start is used to calculate for a hold block prepending the charge block to bee
/// * 'start' - start hour for the proposed charge block
/// * 'soc_level' - the state of charge (SoC) to target the charge block for, it is given from 0-90 (10% is always reserved in the battery)
/// * 'tariffs' - tariffs (sell tariffs used here) inc VAT and extra
/// * 'net_prod' - the net production (production minus consumption) per hour
/// * 'charge_in' - residual charge from previous block
/// * 'charge_tariff_in' - mean tariff for residual charge
/// * 'date_time' - the date to stamp on the block
fn seek_charge(initial_start: usize, start: usize, soc_level: usize, tariffs: &Tariffs, net_prod: [f64;24], charge_in: f64, charge_tariff_in: f64, date_time: DateTime<Local>) -> Blocks {

    let (hold_charge_out, hold_charge_tariff_out, overflow, overflow_price) = update_for_pv(BlockType::Hold, initial_start, start, tariffs, net_prod, charge_in, charge_tariff_in);
    let hold = Block {
        block_type: BlockType::Hold,
        date: date_time.date_naive(),
        start_hour: initial_start,
        end_hour: 0,
        size: start - initial_start,
        tariffs: None,
        charge_tariff_in,
        charge_tariff_out: hold_charge_tariff_out,
        price: 0.0,
        charge_in,
        charge_out: hold_charge_out,
        overflow,
        overflow_price,
        soc_in: 10 + (charge_in / SOC_KWH).round().min(90.0) as usize,
        soc_out: 10 + (hold_charge_out / SOC_KWH).round().min(90.0) as usize,
        status: Status::Waiting,
    };

    let need = (soc_level as f64 * SOC_KWH - hold_charge_out) / CHARGE_EFFICIENCY;
    let charge: Block = if need > 0.0 {
        let (c_price, end) = charge_cost_charge_end(start, need, tariffs);
        let c_this_mean = c_price / need;
        let c_mean = need / (need + hold_charge_out) * c_this_mean + hold_charge_out / (need + hold_charge_out) * hold_charge_tariff_out;

        get_charge_block(start, end - start, tariffs, hold_charge_out, hold_charge_tariff_out, hold_charge_out + need, c_mean, c_price, date_time)

    } else {
        get_charge_block(start, 0, tariffs, hold_charge_out, hold_charge_tariff_out, hold_charge_out, hold_charge_tariff_out, 0.0, date_time)
    };

    Blocks {
        schedule_id: 0,
        next_start: charge.start_hour + charge.size,
        next_charge_in: charge.charge_out,
        next_charge_tariff_in: charge.charge_tariff_out,
        next_soc_in: charge.soc_out,
        total_price: hold.price + hold.overflow_price - charge.price,
        blocks: vec![hold, charge],
    }
}

/// Calculates the cost for a given charge from grid at a given start time
/// It also returns the expected end for the charging period
///
/// # Arguments
///
/// * 'start' - start hour for charging from grid
/// * 'charge' - charge in kWh
/// * 'tariffs' - tariffs (sell tariffs used here) inc VAT and extra
fn charge_cost_charge_end(start: usize, charge: f64, tariffs: &Tariffs) -> (f64, usize) {
    let mut hourly_charge: Vec<f64> = Vec::new();
    let rem = charge % CHARGE_KWH_HOUR;

    (0..(charge / CHARGE_KWH_HOUR) as usize).for_each(|_|hourly_charge.push(CHARGE_KWH_HOUR));
    if (rem * 10.0).round() as usize != 0 {
        hourly_charge.push(rem);
    }
    let end = (start + hourly_charge.len()).min(24);
    let c_price = tariffs.buy[start..end].iter()
        .enumerate()
        .map(|(i, t)| hourly_charge[i] * t)
        .sum::<f64>();

    (c_price, end)
}

/// Updates stored charges and how addition from PV (free electricity) effects the mean price for the stored charge.
/// Also, it breaks out any overflow, i.e. charge that exceeds the battery maximum, and the sell price for that overflow
///
/// # Arguments
///
/// * 'block_type' - The block type which is used to indicate how to deal with periods of net consumption
/// * 'start' - block start hour
/// * 'end' - block end hour (non-inclusive)
/// * 'tariffs' - tariffs (sell tariffs used here) inc VAT and extra
/// * 'net_prod' - production minus consumption per hour
/// * 'charge_in' - residual charge from previous block
/// * 'charge_tariff_in' - mean tariff for residual charge
fn update_for_pv(block_type: BlockType, start: usize, end: usize, tariffs: &Tariffs, net_prod: [f64;24], charge_in: f64, charge_tariff_in: f64) -> (f64, f64, f64, f64) {
    let mut min_charge = charge_in;
    let mut hold_level: f64 = 0.0;
    if block_type == BlockType::Hold {
        hold_level = charge_in;
    }

    let mut charge_tariff_out = charge_tariff_in;
    let (charge_out, overflow, overflow_price) = net_prod[start..end].iter()
        .enumerate()
        .fold((charge_in, 0.0f64, 0.0f64), |acc, (i, &np)| {
            let mut efficiency_adjusted: f64 = np;
            if np < 0.0 {
                efficiency_adjusted = np / DISCHARGE_EFFICIENCY;
            }
            let (c, o) = correct_overflow((acc.0 + efficiency_adjusted).max(hold_level));
            min_charge = min_charge.min(c);

            (c, acc.1 + o, acc.2 + tariffs.sell[i] * o)
        });

    // Blend in free charge from PV into the mean tariff for the period
    if charge_out > charge_in {
        charge_tariff_out = charge_in / charge_out * charge_tariff_in;
    }

    // If charge was ever down to zero then mean tariff for what is left in battery has to be zero
    if min_charge == 0.0 {
        charge_tariff_out = 0.0;
    }

    (charge_out, charge_tariff_out, overflow, overflow_price)
}

/// Corrects for overflow, i.e. separates out what can't be stored in battery as overflow
///
/// It returns a tuple with remaining charge and overflow
///
/// # Arguments
///
/// * 'charge' - charge to correct
fn correct_overflow(charge: f64) -> (f64, f64) {
    (charge.min(BAT_KWH), (charge - BAT_KWH).max(0.0))
}

/// Splits tariffs into twp separate vectors,one for buying and one for selling electricity
///
/// # Arguments
///
/// * 'tariffs' - hourly prices from NordPool (excl VAT)
fn split_tariffs(tariffs: &Vec<f64>) -> Tariffs {
    let mut buy: [f64;24] = [0.0;24];
    let mut sell: [f64;24] = [0.0;24];
    tariffs.iter().enumerate().for_each(|(i, &t)| (buy[i], sell[i]) = add_vat_markup(t));

    Tariffs { buy, sell, }
}

/// Adds VAT and other markups such as energy taxes etc.
///
/// The function spits out one buy price and one sell price
/// Buy:
/// * - Net fee: 31.6 öre (inc VAT)
/// * - Spot fee: 7.7% (inc VAT)
/// * - Energy taxes: 54.875 öre (inc VAT)
/// * - Spot price (excl VAT)
/// * - Extra: 2.4 öre (excl VAT)
///
/// Sell:
/// * - Extra: 9.4 öre (inc VAT)
/// * - Tax reduction: 60 öre (inc VAT), is returned yearly together with tax regulation
/// * - Spot price (excl VAT)
///
/// # Arguments
///
/// * 'tariff' - spot fee as from NordPool
fn add_vat_markup(tariff: f64) -> (f64, f64) {
    let buy = 0.316 + 0.077 * tariff + 0.54875 + (tariff + 0.024) / 0.8;
    let sell = 0.094 + 0.6 + tariff / 0.8;

    (buy, sell * SELL_PRIORITY)
}

/// Creates a new schedule including updating charge levels
///
/// # Arguments
///
/// * 'nordpool' - reference to a NordPool struct
/// * 'smhi' - reference to a SMHI struct
/// * 'date_time' - the date for which the schedule shall be created
/// * 'charge_in' - residual charge from previous block
/// * 'charge_tariff_in' - mean tariff for residual charge
/// * 'backup_dir' - the path to the backup directory
pub fn create_new_schedule(nordpool: &NordPool, smhi: &mut SMHI, pv_diagram: [f64;1440], date_time: DateTime<Local>, charge_in: f64, charge_tariff_in: f64, backup_dir: &str) -> Result<Schedule, SchedulingError> {
    let forecast = retry!(||smhi.new_forecast(date_time))?;
    let production = PVProduction::new(&forecast, LAT, LONG, pv_diagram, date_time);
    let consumption = Consumption::new(&forecast);
    let tariffs = retry!(||nordpool.get_tariffs(date_time))?;
    let schedule = Schedule::new_with_scheduling(date_time.hour() as usize, &tariffs, &production, &consumption, charge_in, charge_tariff_in, date_time);
    save_base_data(backup_dir, date_time, forecast, production.get_production(), consumption.get_consumption())?;

    Ok(schedule)
}
