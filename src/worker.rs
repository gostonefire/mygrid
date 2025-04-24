use std::thread;
use chrono::{DateTime, Datelike, Local, Timelike, Duration, TimeDelta};
use crate::manager_fox_cloud::Fox;
use crate::{retry, wrapper, DEBUG_MODE, MANUAL_DAY};
use crate::backup::{save_last_charge, save_active_block, save_yesterday_statistics};
use crate::charge::{get_last_charge, update_last_charge, updated_charge_data, LastCharge};
use crate::config::Config;
use crate::errors::MyGridWorkerError;
use crate::initialization::Mgr;
use crate::manager_mail::Mail;
use crate::scheduling::{update_schedule, Block, BlockType, Schedule, Status};
use crate::manual::check_manual;

pub fn run(config: Config, mgr: &mut Mgr, mut last_charge: Option<LastCharge>, mut active_block: Option<Block>)
           -> Result<(), MyGridWorkerError> {

    let mut charge_check_done: DateTime<Local> = DateTime::default();
    let mut local_now: DateTime<Local>;
    let mut day_of_year: Option<u32> = None;

    loop {
        thread::sleep(std::time::Duration::from_secs(10));
        local_now = Local::now();

        // Check if we should go into manual mode for today
        if let Some(manual_mode) = check_manual(&config.files.manual_file, local_now)? {
            if manual_mode {
                print_msg("Manual mode activated for today", "Update", None);
            } else {
                print_msg("Manual mode deactivated for today", "Update", None);
            }
        }

        // Check inverter time and save some stats once every day, hour 15 is arbitrary
        if (day_of_year.is_none() || day_of_year.is_some_and(|d| d != local_now.ordinal0())) && local_now.hour() >= 15 {
            check_inverter_local_time(&mgr.fox)?;
            save_yesterday_statistics(&config.files.stats_dir, &mgr.fox)?;
            day_of_year = Some(local_now.ordinal0());
        }

        // Reset last_charge if it is older than 23 hours
        if last_charge.as_ref().is_some_and(|b| local_now - b.date_time_end > TimeDelta::hours(23)) {
            last_charge = None;
        }

        // The inverter seems to discard PV power when in force charge mode and the max SoC
        // has been reached. Hence, we need to check every five minutes (Fox Cloud is updated
        // with that frequency) if we have a started and running charge block where max SoC
        // has been reached. If so, we disable force charge and set the inverter min soc
        // on grid to max soc (i.e. we set it to Hold) and also set the block status to Full.
        if active_block.as_ref().is_some_and(|b| b.is_charge() && b.is_active(local_now)) {

            if local_now - charge_check_done > Duration::minutes(5) {
                let mut block = active_block.unwrap();
                if let Some(status) = set_full_if_done(&mgr.fox, block.soc_out)? {
                    mgr.schedule.update_block(&mut block, status);
                    last_charge = Some(get_last_charge(&block, local_now));

                    save_last_charge(&config.files.backup_dir, &last_charge)?;
                    save_active_block(&config.files.backup_dir, &block)?;
                }
                active_block = Some(block);
                charge_check_done = local_now;
            }
        }

        // This is the main mode selector given a new schedule after every finished block
        if is_update_time(&active_block, local_now) {
            last_charge = update_last_charge(&mgr.schedule, &config.files.backup_dir, &mut active_block, last_charge, get_soc(&mgr.fox)?, local_now)?;
            let (charge_in, charge_tariff_in) = updated_charge_data(&mgr.fox, &active_block, &last_charge, config.charge.soc_kwh)?;

            update_schedule(mgr, local_now, charge_in, charge_tariff_in, &config.files.backup_dir)?;
            
            let mut block = mgr.schedule.get_block(local_now)?;

            let status: Status;
            match block.block_type {
                BlockType::Charge => {
                    status = set_charge(&mgr.fox, &block).map_err(|e| {
                        MyGridWorkerError::new(e.to_string(), &block)
                    })?;
                },

                BlockType::Hold => {
                    status = set_hold(&mgr.fox, block.soc_in as u8).map_err(|e| {
                        MyGridWorkerError::new(e.to_string(), &block)
                    })?;
                },

                BlockType::Use => {
                    status = set_use(&mgr.fox).map_err(|e| {
                        MyGridWorkerError::new(e.to_string(), &block)
                    })?;
                },
            }
            mgr.schedule.update_block_status(local_now, status.clone());
            mgr.schedule.update_block(&mut block, status);
            save_active_block(&config.files.backup_dir, &block)?;
            active_block = Some(block);

            print_schedule(&mgr.schedule,"Update", None);
        }
    }
}

/// Returns true if it is time to update the schedule.
/// This can happen in two occasions:
/// * When the active block is done (it has passed its end hour), or doesn't exist ar all
/// * When the active block has been running for 4 hours (or more), and is not a charge block
///
/// Reason for ending an active block prematurely is that PV power and consumption are estimates
/// given SMHI forecasts on cloud and temperature predictions (which often are inaccurate), and
/// the consumption can vary a lot depending on e.g. cooking and taking showers. Also, the base
/// consumption curve regarding heating is indeed a curve but in practise goes up and down
/// rather unpredictable.
///
/// # Arguments
///
/// * 'active_block' - the block being currently active
/// * 'date_time'- the current date and time
fn is_update_time(active_block: &Option<Block>, date_time: DateTime<Local>) -> bool {
    if !active_block.as_ref().is_some_and(|b| b.is_active(date_time)) {
        true
    } else if active_block.as_ref().is_some_and(|b| { !b.is_charge() && b.get_age(date_time) >= 4 }) {
        true
    } else {
        false
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
        print_msg("Setting inverter time", "Update", None);
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
/// * set the max soc which reflects how much room is needed for PV in following blocks
/// * set the charge schedule
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
/// * 'block' - the configuration to use
fn set_charge(fox: &Fox, block: &Block) -> Result<Status, MyGridWorkerError> {
    print_msg("Setting charge block", "Update", None);
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
/// used as new min soc on grid and finally the max soc is set to 100%
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
/// * 'max_soc' - max soc
fn set_full_if_done(fox: &Fox, max_soc: usize) -> Result<Option<Status>, MyGridWorkerError> {
    let soc= retry!(||fox.get_current_soc())? as usize;
    if soc >= max_soc {
        print_msg("Setting charge block to full", "Update", None);
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
    print_msg("Setting hold block", "Update", None);
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
    print_msg("Setting use block", "Update", None);
    if is_manual_debug()? {return Ok(Status::Started)}

    let _ = retry!(||fox.disable_charge_schedule())?;
    let _ = retry!(||fox.set_min_soc_on_grid(10))?;
    let _ = retry!(||fox.set_max_soc(100))?;

    Ok(Status::Started)
}

/// Retrieves current soc from inverter
/// 
/// # Arguments
/// 
/// * 'fox' - reference to the Fox struct
fn get_soc(fox: &Fox) -> Result<u8, MyGridWorkerError> {
    Ok(retry!(||fox.get_current_soc())?)
}

/// Prints a schedule, i.e. its blocks, with a caption
///
/// # Arguments
///
/// * 'schedule' - the schedule to print
/// * 'caption' - the caption to print
/// * 'mail' - mail sender struct
fn print_schedule(schedule: &Schedule, caption: &str, mail: Option<&Mail>) {
    let report_time = format!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    let caption = format!("{} {} ", report_time, caption);

    let mut msg = format!("{:=<181}\n", caption.to_string() + " ");
    for s in &schedule.blocks {
        msg += &format!("{}\n", s);
    }
    println!("{}", msg);

    if let Some(m) = mail {
        let _ = m.send_mail(caption.to_string(), msg);
    }
}

/// Prints a message with a caption
///
/// # Arguments
///
/// * 'message' - the message
/// * 'caption' - the caption to print
/// * 'mail' - mail sender struct
fn print_msg(message: &str, caption: &str, mail: Option<&Mail>) {
    let report_time = format!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    let caption = format!("{} {} ", report_time, caption);

    let msg = format!("{:=<181}\n{}\n", caption.to_string() + " ", message);
    println!("{}", msg);

    if let Some(m) = mail {
        let _ = m.send_mail(caption.to_string(), msg);
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