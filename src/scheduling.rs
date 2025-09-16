use std::fmt;
use std::fmt::Formatter;
use std::thread;
use chrono::{DateTime, DurationRound, Local, TimeDelta, Timelike};
use serde::{Deserialize, Serialize};
use crate::consumption::ConsumptionValues;
use crate::manager_production::ProductionValues;
use std::collections::HashMap;
use std::ops::Add;
use crate::errors::SchedulingError;
use crate::{retry, wrapper};
use crate::backup::save_base_data;
use crate::config::{ChargeParameters};
use crate::initialization::Mgr;
use crate::models::nordpool_tariffs::TariffValues;

#[derive(Serialize, Deserialize, Clone)]
struct Tariffs {
    buy: [f64;24],
    sell: [f64;24],
    length: usize,
    offset: usize,
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
    pub start_time: DateTime<Local>,
    pub end_time: DateTime<Local>,
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

    /// Return the age in hours between the blocks start and the given date_time
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the date time to get age related to
    pub fn get_age(&self, date_time: DateTime<Local>) -> i64 {
        let date_hour = date_time.duration_trunc(TimeDelta::hours(1)).unwrap();


        (date_hour - self.start_time).num_hours()
    }

    /// Returns true if the block is active in relation to date and time
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the date time the block is valid for
    pub fn is_active(&self, date_time: DateTime<Local>) -> bool {
        let date_hour = date_time.duration_trunc(TimeDelta::hours(1)).unwrap();

        date_hour >= self.start_time && date_hour <= self.end_time
    }

    /// Returns true if the block is a started charge block and not prematurely stopped
    ///
    pub fn is_charge(&self) -> bool {
        self.block_type == BlockType::Charge && self.status == Status::Started
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

/// Struct representing the block schedule from current hour and forward
#[derive(Serialize, Deserialize, Clone)]
pub struct Schedule {
    pub date_time: DateTime<Local>,
    pub blocks: Vec<Block>,
    tariffs: Tariffs,
    bat_capacity: f64,
    bat_kwh: f64,
    soc_kwh: f64,
    charge_kwh_hour: f64,
    charge_efficiency: f64,
    discharge_efficiency: f64,
    sell_priority: f64,
}

impl Schedule {
    /// Creates a new Schedule without scheduling
    /// 
    /// # Arguments
    /// 
    /// * 'config' - configuration struct
    pub fn new(config: &ChargeParameters) -> Schedule {
        Schedule {
            date_time: Default::default(),
            blocks: Vec::new(),
            tariffs: Tariffs {
                buy: [0.0;24],
                sell: [0.0;24],
                length: 0,
                offset: 0,
            },
            bat_capacity: config.bat_capacity,
            bat_kwh: config.bat_kwh,
            soc_kwh: config.soc_kwh,
            charge_kwh_hour: config.charge_kwh_hour,
            charge_efficiency: config.charge_efficiency,
            discharge_efficiency: config.discharge_efficiency,
            sell_priority: config.sell_priority,
        }
    }
    
    /// Updates scheduling based on tariffs, production and consumption estimates.
    /// It can also base the schedule on any residual charge and its mean charge tariff carrying in
    /// from a previous schedule or from the inverter itself (in which case it might be hard to determine
    /// the mean charge tariff).
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - tariffs as given from NordPool
    /// * 'production' - production estimates per hour
    /// * 'consumption' - consumption estimates per hour
    /// * 'charge_in' - any residual charge to bear in to the new schedule
    /// * 'charge_tariff_in' - the mean price for the residual price
    /// * 'date_time' - the date time to stamp on the schedule
    pub fn update_scheduling(&mut self, tariffs: &Vec<TariffValues>, production: &Vec<ProductionValues>, consumption: &Vec<ConsumptionValues>, charge_in: f64, charge_tariff_in: f64, date_time: DateTime<Local>) {
        let date_hour = date_time.duration_trunc(TimeDelta::hours(1)).unwrap();
        let tariffs_in_scope: Vec<(f64,f64)> = tariffs.iter()
            .filter(|t|t.valid_time >= date_hour && t.valid_time < date_hour.add(TimeDelta::days(1)))
            .map(|t|(t.buy, t.sell))
            .collect::<Vec<(f64,f64)>>();
        let allowed_length = tariffs_in_scope.len() as i64;

        let mut prod: [f64;24] = [0.0; 24];
        production.iter()
            .filter(|p|p.valid_time >= date_hour && p.valid_time < date_hour.add(TimeDelta::hours(allowed_length)))
            .enumerate()
            .for_each(|(i, p)| prod[i] = p.power);

        let mut cons: [f64;24] = [0.0; 24];
        consumption.iter()
            .filter(|c|c.valid_time >= date_hour && c.valid_time < date_hour.add(TimeDelta::hours(allowed_length)))
            .enumerate()
            .for_each(|(i, p)| cons[i] = p.power);

        let mut net_prod: [f64;24] = [0.0;24];
        prod.iter()
            .enumerate()
            .for_each(|(i, &p)| net_prod[i] = (p - cons[i]) / 1000.0);

        self.date_time = date_time;
        self.tariffs = self.transform_tariffs(&tariffs_in_scope, date_hour.hour() as usize);
        let blocks = self.seek_best(net_prod, charge_in, charge_tariff_in);
        self.blocks = adjust_for_offset(blocks.blocks, date_hour.hour() as usize);
    }

    /// Updates a block identified by its running hours
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the time to find the block to update for
    /// * 'status' - the status to update with
    pub fn update_block_status(&mut self, date_time: DateTime<Local>, status: Status) {
        let date_hour = date_time.duration_trunc(TimeDelta::hours(1)).unwrap();
        for b in self.blocks.iter_mut() {
            if b.start_time <= date_hour && b.end_time >= date_hour {
                b.status = status;
                return;
            }
        }
    }

    /// Returns a clone of a block identified by hour
    ///
    /// # Arguments
    ///
    /// * 'date_time' - the time to get a block for
    pub fn get_block(&self, date_time: DateTime<Local>) -> Result<Block, SchedulingError> {
        let date_hour = date_time.duration_trunc(TimeDelta::hours(1)).unwrap();
        for b in self.blocks.iter() {
            if b.start_time <= date_hour && b.end_time >= date_hour {
                return Ok(b.clone());
            }
        }

        Err(SchedulingError(format!("no block in schedule corresponding to hour {}", date_hour)))
    }

    /// Seeks the best schedule given input parameters.
    /// The algorithm searches through all combinations of charge blocks, use blocks and charge levels
    /// and returns the one with the best price (i.e. the mean price for usage minus the price for charging).
    /// It also considers charge input from PV, which not only tops up batteries but also lowers the
    /// mean price for the stored energy, which in turn can be used for even lower hourly tariffs.
    ///
    /// # Arguments
    ///
    /// * 'net_prod' - the net production (production minus consumption) per hour
    /// * 'charge_in' - any residual charge to bear in to the new schedule
    /// * 'charge_tariff_in' - the mean price for the residual price
    fn seek_best(&self, net_prod: [f64;24], charge_in: f64, charge_tariff_in: f64) -> Blocks {
        let mut schedule_id: u32 = 0;
        let mut record: HashMap<usize, Blocks> = self.create_base_blocks(schedule_id, charge_in, charge_tariff_in, net_prod);

        for seek_first_charge in 0..self.tariffs.length {
            for charge_level_first in 0..=90 {
                schedule_id += 1;

                let first_charge_blocks = self.seek_charge(0, seek_first_charge, charge_level_first, net_prod, charge_in, charge_tariff_in);
                for seek_first_use in first_charge_blocks.next_start..self.tariffs.length {

                    if let Some(first_use_blocks) = self.seek_use(schedule_id, first_charge_blocks.next_start, seek_first_use, net_prod, first_charge_blocks.next_charge_in, first_charge_blocks.next_charge_tariff_in) {
                        let first_combined = combine_blocks(&first_charge_blocks, &first_use_blocks);
                        self.record_best(1, &first_combined, net_prod, &mut record);

                        for seek_second_charge in first_combined.next_start..self.tariffs.length {
                            for charge_level_second in 0..=90 {
                                schedule_id += 1;

                                let second_charge_blocks = self.seek_charge(first_combined.next_start, seek_second_charge, charge_level_second, net_prod, first_combined.next_charge_in, first_combined.next_charge_tariff_in);
                                for seek_second_use in second_charge_blocks.next_start..self.tariffs.length {

                                    if let Some(second_use_blocks) = self.seek_use(schedule_id, second_charge_blocks.next_start, seek_second_use, net_prod, second_charge_blocks.next_charge_in, second_charge_blocks.next_charge_tariff_in) {
                                        let second_combined = combine_blocks(&second_charge_blocks, &second_use_blocks);
                                        let all_combined = combine_blocks(&first_combined, &second_combined);
                                        self.record_best(2, &all_combined, net_prod, &mut record);
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

    /// Gets charge (and leading hold) block
    ///
    /// # Arguments
    ///
    /// * 'initial_start' - the initial start is used to calculate for a hold block prepending the charge block to bee
    /// * 'start' - start hour for the proposed charge block
    /// * 'soc_level' - the state of charge (SoC) to target the charge block for, it is given from 0-90 (10% is always reserved in the battery)
    /// * 'net_prod' - the net production (production minus consumption) per hour
    /// * 'charge_in' - residual charge from previous block
    /// * 'charge_tariff_in' - mean tariff for residual charge
    fn seek_charge(&self, initial_start: usize, start: usize, soc_level: usize, net_prod: [f64;24], charge_in: f64, charge_tariff_in: f64) -> Blocks {

        let (hold_charge_out, hold_charge_tariff_out, overflow, overflow_price) = self.update_for_pv(BlockType::Hold, initial_start, start, net_prod, charge_in, charge_tariff_in);
        let hold = Block {
            block_type: BlockType::Hold,
            start_time: self.date_time.duration_trunc(TimeDelta::days(1)).unwrap(),
            end_time: self.date_time.duration_trunc(TimeDelta::days(1)).unwrap(),
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
            soc_in: 10 + (charge_in / self.soc_kwh).round().min(90.0) as usize,
            soc_out: 10 + (hold_charge_out / self.soc_kwh).round().min(90.0) as usize,
            status: Status::Waiting,
        };

        let need = (soc_level as f64 * self.soc_kwh - hold_charge_out) / self.charge_efficiency;
        let charge: Block = if need > 0.0 {
            let (c_price, end) = self.charge_cost_charge_end(start, need, None);
            let c_this_mean = c_price / need;
            let c_mean = need / (need + hold_charge_out) * c_this_mean + hold_charge_out / (need + hold_charge_out) * hold_charge_tariff_out;

            self.get_charge_block(start, end - start, hold_charge_out, hold_charge_tariff_out, hold_charge_out + need, c_mean, c_price)

        } else {
            self.get_charge_block(start, 0, hold_charge_out, hold_charge_tariff_out, hold_charge_out, hold_charge_tariff_out, 0.0)
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
    /// * 'tariffs' - optional tariffs to use instead of the registered one in the Schedule struct
    fn charge_cost_charge_end(&self, start: usize, charge: f64, tariffs: Option<&Tariffs>) -> (f64, usize) {
        let mut hourly_charge: Vec<f64> = Vec::new();
        let rem = charge % self.charge_kwh_hour;
        
        let use_tariffs = if let Some(tariffs) = tariffs {
            tariffs
        } else {
            &self.tariffs
        };

        (0..(charge / self.charge_kwh_hour) as usize).for_each(|_|hourly_charge.push(self.charge_kwh_hour));
        if (rem * 10.0).round() as usize != 0 {
            hourly_charge.push(rem);
        }
        let end = (start + hourly_charge.len()).min(use_tariffs.length);
        let c_price = use_tariffs.buy[start..end].iter()
            .enumerate()
            .map(|(i, t)| hourly_charge[i] * t)
            .sum::<f64>();

        (c_price, end)
    }
    
    /// Creates a charge block
    ///
    /// # Arguments
    ///
    /// * 'start' - the charge block starting hour
    /// * 'size' - length of charge block
    /// * 'charge_in' - residual charge from previous block
    /// * 'charge_tariff_in' - mean tariff for residual charge
    /// * 'charge_out' - charge out after charging
    /// * 'charge_tariff_out' - mean tariff for charge in charge block
    /// * 'price' - the price, or cost, for charging
    fn get_charge_block(&self, start: usize, size: usize, charge_in: f64, charge_tariff_in: f64, charge_out: f64, charge_tariff_out: f64, price: f64) -> Block {
        Block {
            block_type: BlockType::Charge,
            start_time: self.date_time.duration_trunc(TimeDelta::days(1)).unwrap(),
            end_time: self.date_time.duration_trunc(TimeDelta::days(1)).unwrap(),
            start_hour: start,
            end_hour: 0,
            size,
            tariffs: Some(self.tariffs.clone()),
            charge_tariff_in,
            charge_tariff_out,
            price,
            charge_in,
            charge_out,
            overflow: 0.0,
            overflow_price: 0.0,
            soc_in: 10 + (charge_in / self.soc_kwh).round().min(90.0) as usize,
            soc_out: 10 + (charge_out / self.soc_kwh).round().min(90.0) as usize,
            status: Status::Waiting,
        }
    }

    /// Seeks a use block
    ///
    /// # Arguments
    ///
    /// * 'schedule_id' - an id that can be later used if some debugging is needed
    /// * 'initial_start' - the initial start is used to calculate for a hold block prepending the charge block to bee
    /// * 'seek_start' - where this run is supposed to start its search
    /// * 'net_prod' - the net production (production minus consumption) per hour
    /// * 'charge_in' - residual charge from previous block
    /// * 'charge_tariff_in' - mean tariff for residual charge
    fn seek_use(&self, schedule_id: u32, initial_start: usize, seek_start: usize, net_prod: [f64;24], charge_in: f64, charge_tariff_in: f64) -> Option<Blocks> {

        for u_start in seek_start..self.tariffs.length {
            // for the hold phase
            let (hold_charge_out, hold_charge_tariff_out, hold_overflow, hold_overflow_price) = self.update_for_pv(BlockType::Hold, initial_start, u_start, net_prod, charge_in, charge_tariff_in);
            let hold = Block {
                block_type: BlockType::Hold,
                start_time: self.date_time.duration_trunc(TimeDelta::days(1)).unwrap(),
                end_time: self.date_time.duration_trunc(TimeDelta::days(1)).unwrap(),
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
                soc_in: 10 + (charge_in / self.soc_kwh).round().min(90.0) as usize,
                soc_out: 10 + (hold_charge_out / self.soc_kwh).round().min(90.0) as usize,
                status: Status::Waiting,
            };
            let mut charge_out:f64 = hold_charge_out;
            let mut charge_tariff_out:f64 = hold_charge_tariff_out;
            let mut overflow:f64 = hold_overflow;
            let mut overflow_price:f64 = hold_overflow_price;
            for u_end in u_start..=self.tariffs.length {
                if u_end > self.tariffs.length - 1 || self.tariffs.buy[u_end] <= charge_tariff_out {
                    if u_end != u_start {
                        return Some(self.get_use_blocks(schedule_id, u_start, u_end, charge_out, charge_tariff_out, overflow, overflow_price, hold, net_prod));
                    }
                    break;
                }

                (charge_out, charge_tariff_out, overflow, overflow_price) = self.update_for_pv(BlockType::Use, u_start, u_end+1, net_prod, hold_charge_out, hold_charge_tariff_out);

                if charge_out.round() == 0.0 {
                    if u_end != u_start {
                        return Some(self.get_use_blocks(schedule_id, u_start, u_end + 1, charge_out, charge_tariff_out, overflow, overflow_price, hold, net_prod));
                    }
                    break;
                }
            }
        }
        None
    }

    /// Creates a use Blocks struct
    ///
    /// # Arguments
    ///
    /// * 'schedule_id' - an id that can be later used if some debugging is needed
    /// * 'u_start' - the use block starting hour
    /// * 'u_end' - the use block end hour (non-inclusive)
    /// * 'charge_out' - residual charge from use block to create
    /// * 'charge_tariff_out' - mean tariff for residual charge from the use block
    /// * 'overflow' - overflow from the use block to create
    /// * 'overflow_price' - price for the overflow from the use block
    /// * 'hold' - the hold block between the charge and use block
    /// * 'net_prod' - the net production (production minus consumption) per hour
    fn get_use_blocks(&self, schedule_id: u32, u_start: usize, u_end: usize, charge_out: f64, charge_tariff_out: f64, overflow: f64, overflow_price: f64, hold: Block, net_prod: [f64;24]) -> Blocks {
        let u_price = net_prod[u_start..u_end].iter()
            .zip(self.tariffs.buy[u_start..u_end].iter())
            .map(|(&np, &t)| np.min(0.0).abs()*t).sum::<f64>();

        let usage = Block {
            block_type: BlockType::Use,
            start_time: self.date_time.duration_trunc(TimeDelta::days(1)).unwrap(),
            end_time: self.date_time.duration_trunc(TimeDelta::days(1)).unwrap(),
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
            soc_out: 10 + (charge_out / self.soc_kwh).round().min(90.0) as usize,
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

    /// Creates an initial base block as a backstop if the search doesn't find any charge/use
    /// opportunities.
    ///
    /// # Arguments
    ///
    /// * 'schedule_id' - an id that can be later used if some debugging is needed
    /// * 'charge_in' - residual charge from previous block (or from previous day)
    /// * 'charge_tariff_in' - mean tariff for residual charge
    /// * 'net_prod' - the net production (production minus consumption) per hour
    fn create_base_blocks(&self, schedule_id: u32, charge_in: f64, charge_tariff_in: f64, net_prod: [f64;24]) -> HashMap<usize, Blocks> {
        let mut record: HashMap<usize, Blocks> = HashMap::new();

        let (charge_out, charge_tariff_out, overflow, _) = self.update_for_pv(BlockType::Use, 0, self.tariffs.length, net_prod, charge_in, charge_tariff_in);

        let block = Block {
            block_type: BlockType::Use,
            start_time: self.date_time.duration_trunc(TimeDelta::days(1)).unwrap(),
            end_time: self.date_time.duration_trunc(TimeDelta::days(1)).unwrap(),
            start_hour: 0,
            end_hour: self.tariffs.length - 1,
            size: self.tariffs.length,
            tariffs: None,
            charge_tariff_in,
            charge_tariff_out,
            price: 0.0,
            charge_in,
            charge_out,
            overflow,
            overflow_price: 0.0,
            soc_in: 10 + (charge_in / self.soc_kwh).round().min(90.0) as usize,
            soc_out: 10 + (charge_out / self.soc_kwh).round().min(90.0) as usize,
            status: Status::Waiting,
        };

        record.insert(0,Blocks {
            schedule_id,
            next_start: self.tariffs.length,
            next_charge_in: block.charge_out,
            next_charge_tariff_in: block.charge_tariff_out,
            next_soc_in: block.soc_out,
            total_price: block.price,
            blocks: vec![block],
        });

        record
    }

    /// Saves the given Blocks struct if the total price is better than any stored for the level
    ///
    /// # Arguments
    ///
    /// * 'level' - level is 1 or 2 and indicates whether it is a first search result or a combined 2-step search
    /// * 'blocks' - the Blocks struct to check and potentially save as the best for its level
    /// * 'net_prod' - the net production (production minus consumption) per hour
    /// * 'record' - the record of the best Blocks structs saved so far
    fn record_best(&self, level: usize, blocks: &Blocks, net_prod: [f64;24], record: &mut HashMap<usize, Blocks>) {
        if let Some(recorded_blocks) = record.get(&level) {
            if blocks.total_price > recorded_blocks.total_price {
                record.insert(level, self.trim_and_tail(blocks, net_prod));
            }
        } else {
            record.insert(level, self.trim_and_tail(blocks, net_prod));
        }
    }

    /// Trims out any blocks with zero size (they are just artifacts from the search flow).
    /// Also, it makes sure that we fill any empty tail with a suitable hold block
    ///
    /// # Arguments
    ///
    /// * 'blocks' - the Blocks struct to trim and add tail to
    /// * 'net_prod' - the net production (production minus consumption) per hour
    fn trim_and_tail(&self, blocks: &Blocks, net_prod: [f64;24]) -> Blocks {
        let mut result = blocks.clone();

        // Trim blocks with no length
        result.blocks = result.blocks.iter().filter(|b| b.size > 0).cloned().collect::<Vec<Block>>();

        let (charge_out, charge_tariff_out, overflow, _) = self.update_for_pv(BlockType::Hold, result.next_start, self.tariffs.length, net_prod, result.next_charge_in, result.next_charge_tariff_in);

        if result.next_start < self.tariffs.length {
            result.blocks.push({
                Block {
                    block_type: BlockType::Hold,
                    start_time: self.date_time.duration_trunc(TimeDelta::days(1)).unwrap(),
                    end_time: self.date_time.duration_trunc(TimeDelta::days(1)).unwrap(),
                    start_hour: result.next_start,
                    end_hour: 0,
                    size: self.tariffs.length - result.next_start,
                    tariffs: None,
                    charge_tariff_in: result.next_charge_tariff_in,
                    charge_tariff_out,
                    price: 0.0,
                    charge_in: result.next_charge_in,
                    charge_out,
                    overflow,
                    overflow_price: 0.0,
                    soc_in: result.next_soc_in,
                    soc_out: 10 + (charge_out / self.soc_kwh).round().min(90.0) as usize,
                    status: Status::Waiting,
                }
            });
            result.next_start = self.tariffs.length;
            result.next_charge_in = charge_out;
            result.next_charge_tariff_in = charge_tariff_out;
            result.next_soc_in = 10 + (charge_out / self.soc_kwh).round().min(90.0) as usize;
        }
        result
    }

    /// Updates stored charges and how addition from PV (free electricity) effects the mean price for the stored charge.
    /// Also, it breaks out any overflow, i.e. charge that exceeds the battery maximum, and the sell price for that overflow
    ///
    /// # Arguments
    ///
    /// * 'block_type' - The block type which is used to indicate how to deal with periods of net consumption
    /// * 'start' - block start hour
    /// * 'end' - block end hour (non-inclusive)
    /// * 'net_prod' - production minus consumption per hour
    /// * 'charge_in' - residual charge from previous block
    /// * 'charge_tariff_in' - mean tariff for residual charge
    fn update_for_pv(&self, block_type: BlockType, start: usize, end: usize, net_prod: [f64;24], charge_in: f64, charge_tariff_in: f64) -> (f64, f64, f64, f64) {
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
                    efficiency_adjusted = np / self.discharge_efficiency;
                }
                let (c, o) = self.correct_overflow((acc.0 + efficiency_adjusted).max(hold_level));
                min_charge = min_charge.min(c);

                (c, acc.1 + o, acc.2 + self.tariffs.sell[i] * o)
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
    fn correct_overflow(&self, charge: f64) -> (f64, f64) {
        (charge.min(self.bat_kwh), (charge - self.bat_kwh).max(0.0))
    }

    /// Prepares tariffs for offset management and factors in sell priority
    ///
    /// # Arguments
    ///
    /// * 'tariffs' - hourly prices from NordPool (excl VAT)
    /// * 'offset' - the offset between first value in the arrays and actual start time
    fn transform_tariffs(&self, tariffs: &Vec<(f64, f64)>, offset: usize) -> Tariffs {
        let mut buy: [f64;24] = [0.0;24];
        let mut sell: [f64;24] = [0.0;24];
        tariffs.iter()
            .enumerate()
            .for_each(|(i, &t)| {
                buy[i] = t.0;
                sell[i] = t.1 * self.sell_priority;
            });

        Tariffs { buy, sell, length: tariffs.len(), offset }
    }

    /// Updates the given block with a new status
    /// If the block is a Charge block and the new status is Full then a new charge tariff out is
    /// calculated. Also, the soc out and charge out is updated to reflect the actual soc.
    ///
    /// # Arguments
    ///
    /// * 'block' - the block to update
    /// * 'status' - the status to update with
    pub fn update_block(&self, block: &mut Block, status: Status) {
        if block.block_type == BlockType::Charge {
            if let Status::Full(soc) = status {
                if soc > block.soc_in {
                    let tariffs = block.tariffs.as_ref().unwrap();
                    let charge_in_bat = (soc - block.soc_in) as f64 * self.soc_kwh;
                    let (cost, _) = self.charge_cost_charge_end(block.start_hour - tariffs.offset, charge_in_bat / self.charge_efficiency, Some(tariffs));
                    let charge_tariff = cost / charge_in_bat;
                    block.charge_tariff_out = ((block.soc_in - 10) as f64 * block.charge_tariff_in + (soc - block.soc_in) as f64 * charge_tariff) / (soc - 10) as f64;
                } else {
                    block.charge_tariff_out = block.charge_tariff_in;
                }
                block.soc_out = soc;
                block.charge_out = (soc - 10) as f64 * self.soc_kwh;
            }
        }
        block.status = status;
    }
}

/// Adjusts start and end hours according the given offset
///
/// # Arguments
///
/// * 'blocks' - a vector of Block
/// * 'offset' - the offset to apply
fn adjust_for_offset(mut blocks: Vec<Block>, offset: usize) -> Vec<Block> {
    for b in blocks.iter_mut(){
        b.start_hour += offset;
        if b.start_hour > 23 {
            b.start_hour -= 24;
            b.start_time = b.start_time.add(TimeDelta::days(1));
        }
        b.end_hour += offset;
        if b.end_hour > 23 {
            b.end_hour -= 24;
            b.end_time = b.end_time.add(TimeDelta::days(1));
        }
        b.start_time = b.start_time.with_hour(b.start_hour as u32).unwrap();
        b.end_time = b.end_time.with_hour(b.end_hour as u32).unwrap();
    }

    blocks
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

/// Updates schedule to reflect current time and circumstances
///
/// # Arguments
///
/// * 'mgr' - reference to the Mgr struct holding all managers
/// * 'date_time' - the date for which the schedule shall be created
/// * 'charge_in' - residual charge from previous block
/// * 'charge_tariff_in' - mean tariff for residual charge
/// * 'backup_dir' - backup directory
pub fn update_schedule(mgr: &mut Mgr, date_time: DateTime<Local>, charge_in: f64, charge_tariff_in: f64, backup_dir: &str) -> Result<(), SchedulingError> {
    let forecast = retry!(||mgr.forecast.new_forecast(date_time))?;
    let (production, production_kw) = mgr.pv.estimate(&forecast)?;
    let consumption = mgr.cons.new_estimates(&forecast);
    let tariffs = retry!(||mgr.nordpool.get_tariffs(date_time))?;
    mgr.schedule.update_scheduling(&tariffs, &production, consumption, charge_in, charge_tariff_in, date_time);
    save_base_data(backup_dir, date_time, &forecast, &production, &production_kw, consumption, &tariffs)?;

    Ok(())
}
