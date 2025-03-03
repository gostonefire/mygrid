use std::thread;
use std::time::Duration;
use chrono::{DateTime, Datelike, Local, Timelike};
use crate::manager_fox_cloud::Fox;
use crate::manager_nordpool::NordPool;
use crate::manager_smhi::SMHI;
use crate::{retry, wrapper};
use crate::scheduling::{create_new_schedule, save_schedule, update_existing_schedule, Block, BlockType, Schedule, Status};

pub fn run(fox: Fox, nordpool: NordPool, smhi: SMHI, mut schedule: Schedule, backup_dir: String)
    -> Result<(), String> {

    // Main loop that runs once every ten seconds
    let mut local_now: DateTime<Local>;
    let mut day_of_year = schedule.date.ordinal0();
    loop {
        thread::sleep(Duration::from_secs(10));
        local_now = Local::now();

        // Create a new schedule everytime we go into a new day
        if day_of_year != local_now.ordinal0() {
            check_inverter_local_time(&fox);
            match create_new_schedule(&nordpool, &smhi, None) {
                Ok(s) => {
                    schedule = s;
                    day_of_year = local_now.ordinal0();
                },
                Err(e) => {
                    eprintln!("Error creating new schedule for day 0: {}", e);
                }
            }
        }

        // The inverter seems to discard PV power when in force charge mode and the max SoC
        // has been reached. Hence, we need to check every five minutes (Fox Cloud is updated
        // with that frequency) if we have a started and running charge block where max SoC
        // has been reached. If so, we disable force charge and set the inverter min soc
        // on grid to max soc (i.e. we set it to Hold) and also set the block status to Full.
        if let Some(b) = schedule.get_current_started_charge(local_now.hour() as u8) {
            if local_now.minute() % 5 == 0 {
                match set_full_if_done(&fox, schedule.blocks[b].max_soc) {
                    Ok(Some(status)) => {
                        schedule.update_block_status(b, status)?;
                        save_schedule(&schedule, &backup_dir);
                    }
                    Err(e) => {
                        print_error(local_now, e, None);
                    }
                    _ => ()
                }
            }
        }

        // This is the main mode switch following the schedule
        if let Some(b) = schedule.get_eligible_for_start(local_now.hour() as u8) {
            let status: Status;
            let mut block = schedule.get_block_clone(b).unwrap();
            match block.block_type {
                BlockType::Charge => {
                    // To ensure we have the best charge level estimate we update the schedule
                    // given the latest forecast from SMHI.
                    update_existing_schedule(&mut schedule, &smhi);
                    block = schedule.get_block_clone(b).unwrap();

                    status = match set_charge(&fox, &block) {
                        Ok(s) => s,
                        Err(e) => {
                            print_error(local_now, e, Some(&block));
                            Status::Error
                        }
                    };
                },
                BlockType::Hold => {
                    status = match set_hold(&fox, block.max_min_soc) {
                        Ok(s) => s,
                        Err(e) => {
                            print_error(local_now, e, Some(&block));
                            Status::Error
                        }
                    };
                },
                BlockType::Use => {
                    status = match set_use(&fox) {
                        Ok(s) => s,
                        Err(e) => {
                            print_error(local_now, e, Some(&block));
                            Status::Error
                        }
                    };
                },
            }
            schedule.update_block_status(b, status)?;

            // Save current schedule version
            save_schedule(&schedule, &backup_dir);
            for s in &schedule.blocks {
                println!("{}", s);
            }
            println!("Update =========================================================================");
        }
    }
}

/// Prints error message for block starts
///
/// # Arguments
///
/// * 'start_time' - time when block started to be set
/// * 'error' - error reported from block start function
/// * 'block' - block that caused the error
fn print_error(start_time: DateTime<Local>, error: String, block: Option<&Block>) {
    let start_time = format!("{}", start_time.format("%Y-%m-%d %H:%M:%S"));
    let report_time = format!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    eprintln!("================================================================================");
    eprintln!("Start Time: {}, Report Time: {}", start_time, report_time);
    eprintln!("Unrecoverable error while setting block: {}", error);
    eprintln!("Block:\n{}", block.map_or_else(|| "None".to_string(), |b| b.to_string()));
}

/// checks the local clock in the inverter and sets it correctly if it has drifted more than a minute
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
fn check_inverter_local_time(fox: &Fox) {
    let err: String;
    match retry!(||fox.get_device_time()) {
        Ok(dt) => {
            let now = Local::now().naive_local();
            let delta = (now - dt).abs();

            if delta > chrono::Duration::minutes(1) {
                match fox.set_device_time(now) {
                    Ok(_) => { return },
                    Err(e) => { err = e }
                }
            } else {
                return;
            }
        },
        Err(e) => { err = e }
    }
    eprintln!("Error getting inverter device time: {}", err);
    eprintln!("This is recoverable unless it repeats to many times")

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
fn set_charge(fox: &Fox, block: &Block) -> Result<Status, String> {
    let report_time = format!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    println!("{} - Setting charge block: maxSoC: {}, start: {}, end: {}",report_time, block.max_soc, block.start_hour, block.end_hour);

    let soc = retry!(||fox.get_current_soc())?;
    if soc >= block.max_soc {
        let _ = retry!(||fox.disable_charge_schedule())?;
        let _ = retry!(||fox.set_min_soc_on_grid(block.max_soc))?;
        let _ = retry!(||fox.set_max_soc(100))?;

        Ok(Status::Full)
    } else {
        let _ = retry!(||fox.set_max_soc(block.max_soc))?;
        let _ = retry!(||fox.set_battery_charging_time_schedule(
                        true, block.start_hour, 0, block.end_hour, 59,
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
fn set_full_if_done(fox: &Fox, max_soc: u8) -> Result<Option<Status>, String> {
    let soc= retry!(||fox.get_current_soc())?;
    if soc >= max_soc {
        let report_time = format!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
        println!("{} - Setting charge block to full",report_time);

        let min_soc = max_soc.max(10).min(100);

        let _ = retry!(||fox.disable_charge_schedule())?;
        let _ = retry!(||fox.set_min_soc_on_grid(min_soc))?;
        let _ = retry!(||fox.set_max_soc(100))?;

        Ok(Some(Status::Full))
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
fn set_hold(fox: &Fox, max_min_soc: u8) -> Result<Status, String> {
    let report_time = format!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    println!("{} - Setting hold block",report_time);

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
fn set_use(fox: &Fox) -> Result<Status, String> {
    let report_time = format!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    println!("{} - Setting use block",report_time);

    let _ = retry!(||fox.disable_charge_schedule())?;
    let _ = retry!(||fox.set_min_soc_on_grid(10))?;
    let _ = retry!(||fox.set_max_soc(100))?;

    Ok(Status::Started)
}
