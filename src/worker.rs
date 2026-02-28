use std::{fs, thread};
use std::ops::Add;
use chrono::{DateTime, Duration, Utc};
use log::info;
use anyhow::Result;
use crate::manager_fox_cloud::Fox;
use crate::{retry, wrapper, DEBUG_MODE, MANUAL_DAY};
use crate::config::Config;
use crate::errors::MyGridWorkerError;
use crate::initialization::Mgr;
use crate::scheduler::{Block, BlockType, FullAt, ImportSchedule, Schedule, Status, BLOCK_UNIT_SIZE};
use crate::manual::check_manual;

pub fn run(config: Config, mgr: &mut Mgr) -> Result<(), MyGridWorkerError> {

    let mut charge_check_done: DateTime<Utc> = DateTime::default();
    let mut utc_now: DateTime<Utc> = mgr.time.utc_now();


    let mut active_block: Option<usize> = mgr.schedule.get_block_by_time(utc_now, false);

    loop {
        thread::sleep(std::time::Duration::from_secs(10));
        utc_now = mgr.time.utc_now();

        // Check if we should go into manual mode for today
        if let Some(manual_mode) = check_manual(&config.files.manual_file, utc_now)? {
            if manual_mode {
                info!("manual mode activated for today");
            } else {
                info!("manual mode deactivated for today");
            }
        }

        // The inverter seems to discard PV power when in force charge mode and the max SoC
        // has been reached. Hence, we need to check every five minutes (Fox Cloud is updated
        // with that frequency) if we have a started and running charge block where max SoC
        // has been reached. If so, we disable force charge and set the inverter min soc
        // on grid to max soc (i.e., we set it to Hold) and also set the block status to Full.
        if let Some(block_id) = active_block {
            if mgr.schedule.is_active_charging(block_id, utc_now)
            {
                let block: &mut Block = mgr.schedule.get_block_by_id(block_id).ok_or("Active block not found")?;
                if utc_now - charge_check_done > Duration::minutes(5) {
                    let soc = get_current_soc(&mgr.fox)?;
                    if let Some(status) = set_full_if_done(&mgr.fox, soc, block.soc_out, utc_now)? {
                        block.update_block_status(status, None);
                        save_schedule_blocks(&config.files.schedule_dir, &mgr.schedule.blocks, mgr.schedule.mode_scheduler)?;
                    }
                    charge_check_done = utc_now;
                }
            }
        }

        // This is the main mode selector
        if active_block.is_none_or(|b| mgr.schedule.is_update_time(b, utc_now))  {

            let block_id = if let Some(block_id) = mgr.schedule.get_block_by_time(utc_now, false) {
                block_id
            } else {
                // If no block is active even after a schedule update, we return the id for an emergency (use) block
                // that will be subject for renewal at each check. Hence, we will try to load a new schedule on every
                // loop until successful.
                mgr.schedule.update_scheduling(utc_now)?;
                let block_id = mgr.schedule.get_block_by_time(utc_now, true)
                    .expect("with fallback shall return block");

                if active_block.is_some_and(|b| b == block_id && block_id == 0) {
                    continue;
                }

                block_id
            };

            let block: &mut Block = mgr.schedule.get_block_by_id(block_id).ok_or("Active block not found")?;

            let status: Status;
            let soc = get_current_soc(&mgr.fox)?;
            match block.block_type {
                BlockType::Charge => {
                    status = set_charge(&mgr.fox, soc, block, utc_now).map_err(|e| {
                        MyGridWorkerError(format!("error set charge: {}", e.to_string()))
                    })?;
                },

                BlockType::Hold => {
                    status = set_hold(&mgr.fox, soc, block.soc_in as u8).map_err(|e| {
                        MyGridWorkerError(format!("error set hold: {}", e.to_string()))
                    })?;
                },

                BlockType::Use => {
                    status = set_use(&mgr.fox).map_err(|e| {
                        MyGridWorkerError(format!("error set use: {}", e.to_string()))
                    })?;
                },
            }
            block.update_block_status(status.clone(), Some(soc));
            log_schedule(&mgr.schedule);

            save_schedule_blocks(&config.files.schedule_dir, &mgr.schedule.blocks, mgr.schedule.mode_scheduler)?;
            active_block = Some(block_id);
        }
    }
}

/// Sets a charge block in the inverter
///
/// The logic is quite simple:
/// * check so max soc is greater than current soc
///     * if not adjust min soc on grid according max soc end return status Full
///     * reason for setting it to max soc is so there is room for estimated PV power
/// * set the max soc which reflects how much room is needed for PV in the following blocks
/// * set the charge schedule
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
/// * 'soc' - current soc
/// * 'block' - the configuration to use
/// * 'utc_now' - current utc time
fn set_charge(fox: &Fox, soc: u8, block: &Block, utc_now: DateTime<Utc>) -> Result<Status, MyGridWorkerError> {
    info!("setting charge block");
    if is_manual_debug()? {return Ok(Status::Started)}

    if soc >= block.soc_out as u8 {
        let _ = retry!(||fox.disable_charge_schedule())?;
        let _ = retry!(||fox.set_min_soc_on_grid(block.soc_out as u8))?;
        let _ = retry!(||fox.set_max_soc(100))?;

        Ok(Status::Full(FullAt {soc: soc as usize, time: utc_now}))
    } else {
        let _ = retry!(||fox.set_max_soc(block.soc_out as u8))?;
        let _ = retry!(||fox.set_battery_charging_time_schedule(
                        true, block.start_time, block.end_time.add(Duration::minutes(BLOCK_UNIT_SIZE)),
                    ))?;

        Ok(Status::Started)
    }
}

/// Sets a charge block to Full in the inverter if it has reached it's max soc
///
/// This is similar to a hold block if the current soc is found to be equal or greater
/// than the given max soc. If so, the charge schedule is disabled, the given max soc is
/// used as a new min soc on grid, and finally the max soc is set to 100%
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
/// * 'soc' - current soc
/// * 'max_soc' - max soc
/// * 'utc_now' - current utc time
fn set_full_if_done(fox: &Fox, soc: u8, max_soc: usize, utc_now: DateTime<Utc>) -> Result<Option<Status>, MyGridWorkerError> {
    if soc as usize >= max_soc {
        info!("setting charge block to full");
        if is_manual_debug()? {return Ok(Some(Status::Full(FullAt {soc: soc as usize, time: utc_now})))}

        let min_soc = max_soc.max(10).min(100);

        let _ = retry!(||fox.disable_charge_schedule())?;
        let _ = retry!(||fox.set_min_soc_on_grid(min_soc as u8))?;
        let _ = retry!(||fox.set_max_soc(100))?;

        Ok(Some(Status::Full(FullAt {soc: soc as usize, time: utc_now})))
    } else {
        Ok(None)
    }
}

/// Sets a hold block in the inverter
///
/// The logic for a hold block is a little busy since there is no equivalent in the inverter:
/// * retrieve the current soc from the invert (given through the soc parameter)
/// * if soc is higher than max min soc, assign max min soc plus half the difference to min soc
///     * this is done since hold blocks after a use block may have used less energy than expected,
///       and in fairness we should give half of that surplus to the next use block which may use
///       more energy than expected.
/// * if soc is lower than max min soc, assign soc to min soc
/// * make sure that we are within global limits, i.e. 10-100
/// * disable any charge block just to make sure that it isn't surviving to the next day
/// * set the min soc on grid in the inverter
/// * set max soc to 100% in the inverter, we don't want to limit anything from PV
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
/// * 'soc' - current soc
/// * 'max_min_soc' - max min soc allowed for the block
fn set_hold(fox: &Fox, soc: u8, max_min_soc: u8) -> Result<Status, MyGridWorkerError> {
    info!("setting hold block");
    if is_manual_debug()? {return Ok(Status::Started)}

    let min_soc = if soc > max_min_soc {
        max_min_soc + (soc - max_min_soc) / 2
    } else {
        soc
    }.clamp(10, 100);

    let _ = retry!(||fox.disable_charge_schedule())?;
    let _ = retry!(||fox.set_min_soc_on_grid(min_soc))?;
    let _ = retry!(||fox.set_max_soc(100))?;

    Ok(Status::Started)
}

/// Sets a use block in the inverter
///
/// The logic for a use block is quite simple:
/// * disable any charge block just to make sure that it isn't surviving to the next day
/// * set the min soc on grid in the inverter to its lowest value of 10%
/// * set max soc to 100% in the inverter, we don't want to limit anything from PV
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
fn set_use(fox: &Fox) -> Result<Status, MyGridWorkerError> {
    info!("setting use block");
    if is_manual_debug()? {return Ok(Status::Started)}

    let _ = retry!(||fox.disable_charge_schedule())?;
    let _ = retry!(||fox.set_min_soc_on_grid(10))?;
    let _ = retry!(||fox.set_max_soc(100))?;

    Ok(Status::Started)
}

/// Returns current State of Charge
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
fn get_current_soc(fox: &Fox) -> Result<u8, MyGridWorkerError> {
    Ok(retry!(||fox.get_current_soc())?)
}

/// Logs a schedule, i.e., its blocks
///
/// # Arguments
///
/// * 'schedule' - the schedule to log
fn log_schedule(schedule: &Schedule) {
    for s in &schedule.blocks {
        info!("{}", s);
    }
}

/// Saves scheduled blocks to file
///
/// # Arguments
///
/// * 'schedule_dir' - the directory to save the file to
/// * 'blocks' - schedule blocks to save
/// * 'mode_scheduler' - whether to use mode scheduler
pub fn save_schedule_blocks(schedule_dir: &str, blocks: &Vec<Block>, mode_scheduler: bool) -> Result<(), MyGridWorkerError> {
    let file_path = format!("{}schedule.json", schedule_dir);

    let import_schedule = ImportSchedule {
        mode_scheduler,
        blocks: blocks.clone(),
    };
    
    let json = serde_json::to_string_pretty(&import_schedule)?;

    fs::write(file_path, json)?;

    Ok(())
}

/// Check if we are in debug mode or manual day
///
fn is_manual_debug() -> Result<bool, MyGridWorkerError> {
    if *DEBUG_MODE.read()? || *MANUAL_DAY.read()? {
        Ok(true)
    } else {
        Ok(false)
    }
}