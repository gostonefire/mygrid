use std::thread;
use chrono::{DateTime, Datelike, Local, Timelike, Duration};
use log::info;
use crate::manager_fox_cloud::Fox;
use crate::{retry, wrapper, DEBUG_MODE, MANUAL_DAY};
use crate::backup::save_schedule_blocks;
use crate::config::Config;
use crate::errors::MyGridWorkerError;
use crate::initialization::Mgr;
use crate::scheduler::{update_schedule, Block, BlockType, Schedule, Status};
use crate::manual::check_manual;

pub fn run(config: Config, mgr: &mut Mgr)
           -> Result<(), MyGridWorkerError> {

    let mut charge_check_done: DateTime<Local> = DateTime::default();
    let mut local_now: DateTime<Local> = Local::now();
    let mut day_of_year: Option<u32> = None;
    let mut active_block: Option<usize> = mgr.schedule.get_block_by_time(local_now);

    loop {
        thread::sleep(std::time::Duration::from_secs(10));
        local_now = Local::now();

        // Check if we should go into manual mode for today
        if let Some(manual_mode) = check_manual(&config.files.manual_file, local_now)? {
            if manual_mode {
                info!("manual mode activated for today");
            } else {
                info!("manual mode deactivated for today");
            }
        }

        // Check inverter time
        if (day_of_year.is_none() || day_of_year.is_some_and(|d| d != local_now.ordinal0())) && local_now.hour() >= 15 {
            check_inverter_local_time(&mgr.fox)?;
            day_of_year = Some(local_now.ordinal0());
        }

        // The inverter seems to discard PV power when in force charge mode and the max SoC
        // has been reached. Hence, we need to check every five minutes (Fox Cloud is updated
        // with that frequency) if we have a started and running charge block where max SoC
        // has been reached. If so, we disable force charge and set the inverter min soc
        // on grid to max soc (i.e., we set it to Hold) and also set the block status to Full.
        if let Some(block_id) = active_block {
            if mgr.schedule.is_active_charging(block_id, local_now)
            {
                let block: &mut Block = mgr.schedule.get_block_by_id(block_id).ok_or("Active block not found")?;
                if local_now - charge_check_done > Duration::minutes(5) {
                    if let Some(status) = set_full_if_done(&mgr.fox, block.soc_out)? {
                        block.update_block_status(status);
                    }
                    charge_check_done = local_now;
                }
            }
        }

        // This is the main mode selector
        if active_block.is_none_or(|b| mgr.schedule.is_update_time(b, local_now))  {

            let block_id = if let Some(block_id) = mgr.schedule.get_block_by_time(local_now) {
                block_id
            } else {
                update_schedule(mgr, local_now, &config.files.backup_dir)?;
                mgr.schedule.get_block_by_time(local_now).ok_or("New schedule is empty for current time")?
            };

            let block: &mut Block = mgr.schedule.get_block_by_id(block_id).ok_or("Active block not found")?;

            let status: Status;
            match block.block_type {
                BlockType::Charge => {
                    status = set_charge(&mgr.fox, block).map_err(|e| {
                        MyGridWorkerError::new(e.to_string(), block)
                    })?;
                },

                BlockType::Hold => {
                    status = set_hold(&mgr.fox, block.soc_in as u8).map_err(|e| {
                        MyGridWorkerError::new(e.to_string(), block)
                    })?;
                },

                BlockType::Use => {
                    status = set_use(&mgr.fox).map_err(|e| {
                        MyGridWorkerError::new(e.to_string(), block)
                    })?;
                },
            }
            block.update_block_status(status.clone());

            save_schedule_blocks(&config.files.backup_dir, &mgr.schedule.blocks)?;
            log_schedule(&mgr.schedule);
            active_block = Some(block_id);
        }
    }
}

/// checks the local clock in the inverter and sets it correctly if it has drifted more than a minute
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
fn check_inverter_local_time(fox: &Fox) -> Result<(), MyGridWorkerError> {
    let dt = retry!(||fox.get_device_time())?;
    let now = Local::now().naive_local();
    let delta = (now - dt).abs();

    if delta > Duration::minutes(1) {
        info!("setting inverter time");
        let _ = fox.set_device_time(now)?;
    }

    Ok(())
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
/// * 'block' - the configuration to use
fn set_charge(fox: &Fox, block: &Block) -> Result<Status, MyGridWorkerError> {
    info!("setting charge block");
    if is_manual_debug()? {return Ok(Status::Started)}

    let soc = retry!(||fox.get_current_soc())?;
    if soc >= block.soc_out as u8 {
        let _ = retry!(||fox.disable_charge_schedule())?;
        let _ = retry!(||fox.set_min_soc_on_grid(block.soc_out as u8))?;
        let _ = retry!(||fox.set_max_soc(100))?;

        Ok(Status::Full(soc as usize))
    } else {
        let _ = retry!(||fox.set_max_soc(block.soc_out as u8))?;
        let _ = retry!(||fox.set_battery_charging_time_schedule(
                        true, block.start_hour as u8, 0, block.end_hour as u8, 59,
                        false, 0, 0, 0, 0,
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
/// * 'max_soc' - max soc
fn set_full_if_done(fox: &Fox, max_soc: usize) -> Result<Option<Status>, MyGridWorkerError> {
    let soc= retry!(||fox.get_current_soc())? as usize;
    if soc >= max_soc {
        info!("setting charge block to full");
        if is_manual_debug()? {return Ok(Some(Status::Full(soc)))}

        let min_soc = max_soc.max(10).min(100);

        let _ = retry!(||fox.disable_charge_schedule())?;
        let _ = retry!(||fox.set_min_soc_on_grid(min_soc as u8))?;
        let _ = retry!(||fox.set_max_soc(100))?;

        Ok(Some(Status::Full(soc)))
    } else {
        Ok(None)
    }
}

/// Sets a hold block in the inverter
///
/// The logic for a hold block is a little busy since there is no equivalent in the inverter:
/// * retrieve the current soc from the invert
/// * get the lowest of the two values max_min_soc and soc
///     * charge block may have exceeded it with PV power so soc is too high, in which we use max min soc
///     * charge block may have not fully reached max soc, in which case we use current soc
/// * make sure that we are within global limits, i.e. 10-100
/// * disable any charge block just to make sure that it isn't surviving to the next day
/// * set the min soc on grid in the inverter
/// * set max soc to 100% in the inverter, we don't want to limit anything from PV
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
/// * 'max_min_soc' - max min soc allowed for the block
fn set_hold(fox: &Fox, max_min_soc: u8) -> Result<Status, MyGridWorkerError> {
    info!("setting hold block");
    if is_manual_debug()? {return Ok(Status::Started)}

    let soc = retry!(||fox.get_current_soc())?;
    let min_soc = max_min_soc.min(soc).max(10).min(100);

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

/// Check if we are in debug mode or manual day
///
fn is_manual_debug() -> Result<bool, MyGridWorkerError> {
    if *DEBUG_MODE.read()? || *MANUAL_DAY.read()? {
        Ok(true)
    } else {
        Ok(false)
    }
}