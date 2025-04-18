use std::ops::Add;
use std::thread;
use chrono::{DateTime, Local, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use crate::errors::MyGridWorkerError;
use crate::manager_fox_cloud::Fox;
use crate::{retry, wrapper};
use crate::backup::save_last_charge;
use crate::scheduling::{Block, Schedule, Status};


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
fn get_soc_history(fox: &Fox, start_time: Option<DateTime<Local>>) -> Result<(Vec<u8>, u8), MyGridWorkerError> {
    let mut soc_history: Vec<u8> = if let Some(start_time) = start_time {

        let mut start = start_time.with_timezone(&Utc);
        let end =  Local::now().with_timezone(&Utc);
        if end - start >= TimeDelta::hours(24) {
            start = end
                .add(TimeDelta::hours(-24))
                .add(TimeDelta::seconds(1))
        }
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
/// * 'soc_kwh' - kwh per soc unit
fn soc_to_available_charge(soc: u8, soc_kwh: f64) -> f64 {
    (soc.max(10) - 10) as f64 * soc_kwh
}

/// Updates last charge in case the active block is a charge block, otherwise it just
/// returns the same last charge as was given in the call
/// # Arguments
///
/// * 'schedule' - reference to the Schedule struct
/// * 'backup_dir' - the directory to save the new last charge to
/// * 'active_block' - the block that was or is active
/// * 'last_charge' - data from the last finished charge from grid
/// * 'soc' - current SoC to update for
/// * 'date_time' - date time to stamp as the end of the last charge
pub fn update_last_charge(schedule: &Schedule, backup_dir: &str, active_block: &mut Option<Block>, last_charge: Option<LastCharge>, soc: u8, date_time: DateTime<Local>) -> Result<Option<LastCharge>, MyGridWorkerError> {
    if active_block.as_ref().is_some_and(|b| b.is_charge()) {
        let block = active_block.as_mut().unwrap();
        schedule.update_block(block, Status::Full(soc as usize));
        let new_last_charge = Some(get_last_charge(block, date_time));
        save_last_charge(backup_dir, &new_last_charge)?;

        Ok(new_last_charge)
    } else {
        Ok(last_charge)
    }
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

/// Calculate updated charge related values for resent charge-from-grid blocks
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
/// * 'active_block' - the block that was active to be used if last charge isn't provided
/// * 'last_charge' - data from the last finished charge from grid
/// * 'soc_kwh' - kwh per soc unit
pub fn updated_charge_data(fox: &Fox, active_block: &Option<Block>, last_charge: &Option<LastCharge>, soc_kwh: f64) -> Result<(f64, f64), MyGridWorkerError> {
    match last_charge {
        None => {
            let (charge_tariff_out, soc_current) = match active_block {
                None => {
                    let (_, soc_current) = get_soc_history(&fox, None)?;
                    (0.0, soc_current)
                },
                Some(b) if b.charge_tariff_in > 0.0 => {
                    let (soc_history, soc_current) = get_soc_history(&fox, Some(b.start_time))?;
                    let charge_tariff_out = update_stored_charge_cost(&soc_history, b.charge_tariff_in);
                    (charge_tariff_out, soc_current)

                },
                _ => {
                    let (_, soc_current) = get_soc_history(&fox, None)?;
                    (0.0, soc_current)
                }
            };
            Ok((soc_to_available_charge(soc_current, soc_kwh), charge_tariff_out))
        },
        Some(last_charge) => {
            let (soc_history, soc_current) = get_soc_history(&fox, Some(last_charge.date_time_end))?;
            let charge_tariff_out = update_stored_charge_cost(&soc_history, last_charge.charge_tariff_out);

            Ok((soc_to_available_charge(soc_current, soc_kwh), charge_tariff_out))
        }
    }
}

/// Calculates a new cost for energy stored in the battery given a SoC history and a charge tariff
/// as of from the end of the latest charge from grid.
///
/// # Arguments
///
/// * 'soc_history' - the SoC history starting from the end of a battery charge from grid
/// * 'charge_tariff_in' - the charge tariff per kWh as of the end of a battery charge from grid
fn update_stored_charge_cost(soc_history: &Vec<u8>, charge_tariff_in: f64) -> f64 {
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

        // Store value from input if we detected a peak or a valley
        if cmd != ValueType::NA {
            peaks_valleys.push(Value { value_type: cmd, value: soc_history[s] });
        }
    }

    // Calculate tariff (what the stored energy has cost us per kWh) after blending in free power from PV
    // if there is any peak or valley that hits battery min soc (10) then the charge tariff out must
    // be 0.0 since no more usable charge with positive tariff exists.
    if peaks_valleys.iter().any(|v| v.value <= 10) {
        0.0
    } else {
        for (i, v) in peaks_valleys.iter().enumerate() {
            if v.value_type == ValueType::Peak && i > 0 {
                charge_tariff_out = peaks_valleys[i-1].value as f64 * charge_tariff_out / v.value as f64;
            }
        }

        charge_tariff_out
    }
}