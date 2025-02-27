use chrono::{DateTime, Datelike, Local, TimeDelta, Timelike, Utc};
use std::{env, thread};
use std::ops::{Add};
use std::process::exit;
use std::time::Duration;
use crate::consumption::Consumption;
use crate::manager_fox_cloud::Fox;
use crate::manager_nordpool::NordPool;
use crate::manager_smhi::SMHI;
use crate::production::PVProduction;
use crate::scheduling::{Block, BlockType, Schedule, Status};

mod manager_nordpool;
mod manager_fox_cloud;
mod manager_sun;
mod models;
mod manager_smhi;
mod scheduling;
mod production;
mod consumption;
mod macros;

/// Latitude of the power plant
const LAT: f64 = 56.22332313734338;

/// Longitude of the power plant
const LONG: f64 = 15.658393416666142;

fn main() {
    let api_key: String;
    let inverter_sn: String;
    match env::var("FOX_ESS_API_KEY") {
        Ok(v) => api_key = v,
        Err(e) => {eprintln!("Error getting API key: {}", e); return;}
    }
    match env::var("FOX_ESS_INVERTER_SN") {
        Ok(v) => inverter_sn = v,
        Err(e) => {eprintln!("Error getting inverter SN: {}", e); return;}
    }

    // Instantiate structs
    let fox = Fox::new(api_key);
    let nordpool = NordPool::new();
    let smhi = SMHI::new(LAT, LONG);

    // Check and possibly set the local clock in the inverter
    check_inverter_local_time(&fox, &inverter_sn);

    // Create a first base schedule given only tariffs, charge level will later be updated
    let mut schedule: Schedule;
    match create_new_schedule(&nordpool, &smhi, Some(1)) {
        Ok(s) => schedule = s,
        Err(e) => {
            eprintln!("Error creating new schedule while starting up: {}", e);
            exit(1);
        }
    }
    for s in &schedule.blocks {
        println!("{}", s);
    }

    // Main loop that runs once a minute
    let mut local_now: DateTime<Local>;
    let mut day_of_year = Local::now().ordinal0();
    loop {
        thread::sleep(Duration::from_secs(60));
        local_now = Local::now();
        if day_of_year != local_now.ordinal0() {
            check_inverter_local_time(&fox, &inverter_sn);
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

        if let Some(b) = schedule.get_eligible_for_start(local_now.hour() as u8) {
            let mut block = schedule.get_block_clone(b).unwrap();
            match block.block_type {
                BlockType::Charge => {
                    update_existing_schedule(&mut schedule, &smhi);
                    block = schedule.get_block_clone(b).unwrap();

                    if let Err(e) = set_charge(&fox, &inverter_sn, &block) {
                        print_error(local_now, e, &block);
                    }

                },
                BlockType::Hold => {
                    if let Err(e) = set_hold(&fox, &inverter_sn) {
                        print_error(local_now, e, &block);
                    }
                },
                BlockType::Use => {
                    if let Err(e) = set_use(&fox, &inverter_sn) {
                        print_error(local_now, e, &block);
                    }
                },
            }
            schedule.update_block_status(b, Status::Started).unwrap();

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
fn print_error(start_time: DateTime<Local>, error: String, block: &Block) {
    let start_time = format!("{}", start_time.format("%Y-%m-%d %H:%M:%S"));
    let report_time = format!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    eprintln!("================================================================================");
    eprintln!("Start Time: {}, Report Time: {}", start_time, report_time);
    eprintln!("Unrecoverable error while setting block: {}", error);
    eprintln!("Block:\n{}", block)
}

/// checks the local clock in the inverter and sets it correctly if it has drifted more than a minute
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
/// * 'sn' - serial number of the inverter
fn check_inverter_local_time(fox: &Fox, sn: &str) {
    let mut err: String = "".to_string();
    match retry!(||fox.get_device_time(sn)) {
        Ok(dt) => {
            let now = Local::now().naive_local();

            if now - dt > chrono::Duration::minutes(1) {
                match fox.set_device_time(sn, now) {
                    Ok(_) => { return },
                    Err(e) => { err = e }
                }
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
/// * set the max soc which reflects how much room is needed for PV in following blocks
/// * set the charge schedule
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
/// * 'sn' - serial number of the inverter
/// * 'block' - the configuration to use
fn set_charge(fox: &Fox, sn: &str, block: &Block) -> Result<(), String> {
    let _ = retry!(||fox.set_max_soc(sn, block.max_soc))?;
    let _ = retry!(||fox.set_battery_charging_time_schedule(
                        sn,
                        true, block.start_hour, 0, block.end_hour, 59,
                        false, 0, 0, 0, 0,
                    ))?;

    Ok(())
}

/// Sets a hold block in the inverter
///
/// The logic for a hold block is a little busy since there is no equivalent in the inverter:
/// * disable any charge block just to make sure that it isn't surviving to the next day
/// * retrieve the current max soc settings from the inverter
/// * retrieve the current soc from the invert
/// * get the lowest of the two soc values
///     * charge block may have exceeded it with PV power so soc is too high, in which we use max soc
///     * charge block may have not fully reached max soc, in which case we use current soc
/// * make sure that we are within global limits, i.e. 10-100
/// * set the min soc on grid in the inverter
/// * set max soc to 100% in the inverter, we don't want to limit anything from PV
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
/// * 'sn' - serial number of the inverter
fn set_hold(fox: &Fox, sn: &str) -> Result<(), String> {
    let _ = retry!(||fox.set_battery_charging_time_schedule(
                        sn,
                        false, 0, 0, 0, 0,
                        false, 0, 0, 0, 0,
                    ))?;
    let max_soc= retry!(||fox.get_max_soc(sn))?;
    let soc= retry!(||fox.get_current_soc(sn))?;
    let min_soc = max_soc.min(soc).max(10).min(100);
    let _ = retry!(||fox.set_min_soc_on_grid(sn, min_soc))?;
    let _ = retry!(||fox.set_max_soc(sn, 100))?;

    Ok(())
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
/// * 'sn' - serial number of the inverter

fn set_use(fox: &Fox, sn: &str) -> Result<(), String> {
    let _ = retry!(||fox.set_battery_charging_time_schedule(
                        sn,
                        false, 0, 0, 0, 0,
                        false, 0, 0, 0, 0,
                    ))?;
    let _ = retry!(||fox.set_min_soc_on_grid(sn, 10))?;
    let _ = retry!(||fox.set_max_soc(sn, 100))?;

    Ok(())
}

/// Creates a new schedule including updating charge levels
///
/// # Arguments
///
/// * 'nordpool' - reference to a NordPool struct
/// * 'SMHI' - reference to a SMHI struct
/// * 'future' - optional future in days, i.e. if set to 1 it will create a schedule for tomorrow
fn create_new_schedule(nordpool: &NordPool, smhi: &SMHI, future: Option<usize>) -> Result<Schedule, String> {
    let mut d = 0;
    if let Some(f) = future { d = f as i64 }
    let forecast = retry!(||smhi.get_forecast(Local::now().add(TimeDelta::days(d))))?;
    let production = PVProduction::new(&forecast, LAT, LONG);
    let consumption = Consumption::new(&forecast);
    let tariffs = retry!(||nordpool.get_tariffs(Utc::now().add(TimeDelta::days(d))))?;
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
fn update_existing_schedule(schedule: &mut Schedule, smhi: &SMHI) {
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
