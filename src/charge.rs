use std::thread;
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use crate::errors::MyGridWorkerError;
use crate::manager_fox_cloud::Fox;
use crate::{retry, wrapper};
use crate::scheduling::{Block, SOC_KWH};


#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct LastCharge {
    pub date_time_end: DateTime<Local>,
    pub soc_in: usize,
    pub soc_out: usize,
    pub charge_in: f64,
    pub charge_out: f64,
    pub charge_tariff_in: f64,
    pub charge_tariff_out: f64
}

#[derive(Copy, Clone, PartialEq, Debug)]
enum Comp {
    End,
    Larger,
    Smaller,
    Equal,
    NA,
}

#[derive(PartialEq, Debug)]
enum ValueType {
    Peak,
    Valley,
    NA,
}

#[derive(Debug)]
struct Value {
    value_type: ValueType,
    value: u8,
}

/// Retrieves SoC history up until now.
/// It also fetches the current SoC to guarantee that there will always be at least
/// one SoC value (the current) in the returned Vec. The current soc is also
/// separately returned for convenience.
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
/// * 'start_time' - the start time for the history, if None given then no history is returned, just current SoC
pub fn get_soc_history(fox: &Fox, start_time: Option<DateTime<Local>>) -> Result<(Vec<u8>, u8), MyGridWorkerError> {
    let mut soc_history: Vec<u8> = if let Some(start_time) = start_time {
        let start = start_time.with_timezone(&Utc);
        let end =  Local::now().with_timezone(&Utc);
        let device_history = retry!(||fox.get_device_history_data(start, end))?;
        device_history.soc
    } else {
        Vec::new()
    };

    let soc = retry!(|| fox.get_current_soc())?;
    soc_history.push(soc);

    Ok((soc_history, soc))
}

/// Calculates how much energy/charge a certain SoC corresponds to
///
/// # Arguments
///
/// * 'soc' - the state of charge as reported from Fox, i.e. including the 10 not usable
pub fn soc_to_available_charge(soc: u8) -> f64 {
    (soc.max(10) - 10) as f64 * SOC_KWH
}

/// Copies in data from last charging block into a LastCharge struct and stamps it
/// with the time the charging actually ended
///
/// # Arguments
///
/// * 'charge_block' - the charging block that has ended (Full or replaced by another type of block)
/// * 'date_time' - time when the charging phase was ended
pub fn get_last_charge(charge_block: &Block, date_time: DateTime<Local>) -> LastCharge {
    LastCharge {
        date_time_end: date_time,
        soc_in: charge_block.soc_in,
        soc_out: charge_block.soc_out,
        charge_in: charge_block.charge_in,
        charge_out: charge_block.charge_out,
        charge_tariff_in: charge_block.charge_tariff_in,
        charge_tariff_out: charge_block.charge_tariff_out,
    }
}

/// Calculates a new cost for energy stored in the battery given a SoC history and a charge tariff
/// as of from the end of the latest charge from grid.
///
/// # Arguments
///
/// * 'soc_history' - the SoC history starting from the end of a battery charge from grid
/// * 'charge_tariff_in' - the charge tariff per kWh as of the end of a battery charge from grid
pub fn update_stored_charge_cost(soc_history: &Vec<u8>, charge_tariff_in: f64) -> f64 {
    let mut charge_tariff_out: f64 = charge_tariff_in;
    let mut peaks_valleys: Vec<Value> = Vec::new();

    let mut left: Comp;
    let mut right: Comp;
    let mut left_memory: Comp = Comp::NA;

    for s in 0..soc_history.len() {
        let mut cmd: ValueType = ValueType::NA;

        // Compare with value to the left
        if s == 0 {
            left = Comp::End;
        } else if soc_history[s] > soc_history[s-1] {
            left = Comp::Larger;
        } else if soc_history[s] < soc_history[s-1] {
            left = Comp::Smaller;
        } else {
            left = Comp::Equal;
        }

        // Compare with value to the right
        if s == soc_history.len() - 1 {
            right = Comp::End;
        } else if soc_history[s] > soc_history[s+1] {
            right = Comp::Larger;
        } else if soc_history[s] < soc_history[s+1] {
            right = Comp::Smaller;
        } else {
            right = Comp::Equal;
        }

        // Match for saving left to memory (we are on a plateau of some sort), or for peaks or valleys
        match (left, right, left_memory) {
            (Comp::End, Comp::Equal, _) => left_memory = Comp::End,
            (Comp::Larger, Comp::Equal, _) => left_memory = Comp::Larger,
            (Comp::Smaller, Comp::Equal, _) => left_memory = Comp::Smaller,
            (Comp::Equal, Comp::Larger, Comp::Smaller) => left_memory = Comp::NA,
            (Comp::Equal, Comp::Smaller, Comp::Larger) => left_memory = Comp::NA,

            (Comp::End, Comp::Larger, _) => cmd = ValueType::Peak,
            (Comp::Equal, Comp::End, Comp::Larger) => cmd = ValueType::Peak,
            (Comp::Larger, r, _) if r == Comp::Larger || r == Comp::End => cmd = ValueType::Peak,
            (Comp::Equal, Comp::Larger, m) if m == Comp::Larger || m == Comp::End => { cmd = ValueType::Peak; left_memory = Comp::NA; },

            (Comp::End, Comp::Smaller, _) => cmd = ValueType::Valley,
            (Comp::Equal, Comp::End, Comp::Smaller) => cmd = ValueType::Valley,
            (Comp::Smaller, r, _) if r == Comp::Smaller || r == Comp::End => cmd = ValueType::Valley,
            (Comp::Equal, Comp::Smaller, m) if m == Comp::Smaller || m == Comp::End => { cmd = ValueType::Valley; left_memory = Comp::NA; },

            _ => (),
        }

        // Store value form input if we detected a peaks or a valley
        if cmd != ValueType::NA {
            peaks_valleys.push(Value { value_type: cmd, value: soc_history[s] });
        }
    }

    // Calculate tariff (what the stored energy has cost us per kWh) after blending in free power from PV
    for (i, v) in peaks_valleys.iter().enumerate() {
        if v.value_type == ValueType::Peak && i > 0 {
            charge_tariff_out = peaks_valleys[i-1].value as f64 * charge_tariff_out / v.value as f64;
        }
    }

    charge_tariff_out
}