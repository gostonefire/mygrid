use std::collections::{HashSet};
use std::fmt;

const CHARGE_LEN: usize = 3;
const USE_LEN: usize = 5;

#[derive(Clone)]
pub struct Block {
    pub block_type: String,
    pub min_soc_on_grid: Option<f64>,
    pub max_soc: f64,
    pub start_hour: usize,
    pub end_hour: usize,
    pub mean_price: f64,
    pub hour_price: Vec<f64>,
}

impl fmt::Display for Block {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} -> {:>2} - {:>2}: minSoc {}, maxSoc {:<3}, price {:0.3} {:?}",
               self.block_type, self.start_hour, self.end_hour,
               self.min_soc_on_grid.map_or("None".to_string(), |d| format!("{:>4}", d)),
               self.max_soc, self.mean_price, self.hour_price)
    }
}

pub struct Schedule {
    pub blocks: Vec<Block>,
}

pub fn create_schedule(tariffs: &Vec<f64>) -> Schedule {
    let mut schedule = Schedule { blocks: Vec::new() };
    let segments: [(usize,usize);3] = [(0,8 - CHARGE_LEN), (8, 16 - CHARGE_LEN), (16, 24 - CHARGE_LEN)];

    let mut blocks: Vec<Block> = Vec::new();
    for s in segments.iter() {
        let charge_block = get_charge_block(&tariffs, CHARGE_LEN, s.0, s.1);
        blocks = get_use_block(&tariffs, charge_block.mean_price / 0.8, USE_LEN, charge_block.end_hour + 1, 23);
        if !blocks.is_empty() {
            schedule.blocks.push(charge_block);
            schedule.blocks.push(blocks[0].clone());
            break;
        }
    }

    while !blocks.is_empty() {
        if blocks.len() == 1 {
            let charge_block = get_charge_block(&tariffs, CHARGE_LEN, blocks[0].end_hour + 1, 23);
            blocks = get_use_block(&tariffs, charge_block.mean_price / 0.8, USE_LEN, charge_block.end_hour + 1, 23);
            if !blocks.is_empty() {
                schedule.blocks.push(charge_block);
                schedule.blocks.push(blocks[0].clone());
            }

        } else if blocks.len() >= 1 {
            let charge_block = get_charge_block(&tariffs, CHARGE_LEN, blocks[0].end_hour + 1, blocks[1].start_hour - 1);
            blocks = get_use_block(&tariffs, charge_block.mean_price / 0.8, USE_LEN, charge_block.end_hour + 1, 23);
            if !blocks.is_empty() {
                schedule.blocks.push(charge_block);
                schedule.blocks.push(blocks[0].clone());
            }
        }
    }

    add_hold_blocks(schedule, tariffs)
}

fn add_hold_blocks(schedule: Schedule, tariffs: &Vec<f64>) -> Schedule {
    let mut new_schedule = Schedule { blocks: Vec::new() };
    if schedule.blocks.is_empty() {
        new_schedule.blocks.push(create_hold_block(tariffs, 0, 23));
        return new_schedule;
    }

    let mut next_start_hour: usize = 0;
    for block in schedule.blocks {
        if block.start_hour != next_start_hour {
            new_schedule.blocks.push(create_hold_block(tariffs, next_start_hour, block.start_hour - 1));
        }
        next_start_hour = block.end_hour + 1;
        new_schedule.blocks.push(block);
    }

    if next_start_hour != 24 {
        new_schedule.blocks.push(create_hold_block(tariffs, next_start_hour, 23));
    }
    new_schedule
}

fn create_hold_block(tariffs: &Vec<f64>, start: usize, end: usize) -> Block {
    let hour_price = tariffs[start..=end].to_vec();
    Block {
        block_type: "H".to_string(),
        min_soc_on_grid: None,
        max_soc: 100.0,
        start_hour: start,
        end_hour: end,
        mean_price: hour_price.iter().sum::<f64>() / hour_price.len() as f64,
        hour_price,
    }
}

fn get_charge_block(tariffs: &Vec<f64>, block_len: usize, start: usize, end: usize) -> Block {
    let mut block: Block = Block {
        block_type: "C".to_string(),
        min_soc_on_grid: None,
        max_soc: 0.0,
        start_hour: 0,
        end_hour: 0,
        mean_price: 10000.0,
        hour_price: Vec::new(),
    };

    for hour in start..=end.min(24 - block_len) {
        let hour_price: Vec<f64> = tariffs[hour..(hour + block_len)].to_vec();
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

fn get_use_block(tariffs: &Vec<f64>, min_price: f64, max_block_len: usize, start: usize, end: usize) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();

    for start_hour in start..=end {
        if tariffs[start_hour] >= min_price {
            let mut prices: Vec<f64> = vec![tariffs[start_hour]];
            for hour2 in (start_hour + 1)..(start_hour + max_block_len) {
                if tariffs[hour2] >= min_price {
                    prices.push(tariffs[hour2]);
                } else {
                    break;
                }
            }
            blocks.push(Block {
                block_type: "U".to_string(),
                min_soc_on_grid: Some(10.0),
                max_soc: 100.0,
                start_hour,
                end_hour: start_hour + prices.len() - 1,
                //mean_price: prices.iter().map(|&p| p.powi(2)).sum::<f64>() / prices.len() as f64,
                mean_price: prices.iter().sum::<f64>() / prices.len() as f64,
                //mean_price: prices.iter().sum::<f64>(),
                hour_price: prices,
            });
        }
    }

    filter_out_subsets(blocks)
}

fn filter_out_subsets(blocks: Vec<Block>) -> Vec<Block> {
    let mut intermediate_blocks: Vec<Block> = Vec::new();
    let mut filtered_blocks: Vec<Block> = Vec::new();

    let mut set: HashSet<usize> = HashSet::new();
    let mut mean_price: f64 = 0.0;

    // Filter out succeeding blocks that are either subsets of a preceding block or
    // are intersects but with a lower mean price. Also, a succeeding block that is
    // an intersect, but with higher mean price, will replace the preceding block.
    for block in blocks {
        let next_set: HashSet<usize> = (block.start_hour..=block.end_hour).collect::<HashSet<usize>>();
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
    let mut end_hour: usize = 24;
    for block in intermediate_blocks {
        if block.start_hour != end_hour + 1 {
            end_hour = block.end_hour;
            filtered_blocks.push(block);
        }
    }

    filtered_blocks
}
