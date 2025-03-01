use chrono::{DateTime, Datelike, Local, TimeDelta, Timelike};
use std::{env, fs, thread};
use std::fs::File;
use std::io::{Read};
use std::ops::{Add};
use std::process::exit;
use std::time::Duration;
use glob::glob;
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
    let backup_dir: String;
    match env::var("FOX_ESS_API_KEY") {
        Ok(v) => api_key = v,
        Err(e) => {eprintln!("Error getting API key: {}", e); return;}
    }
    match env::var("FOX_ESS_INVERTER_SN") {
        Ok(v) => inverter_sn = v,
        Err(e) => {eprintln!("Error getting inverter SN: {}", e); return;}
    }
    match env::var("BACKUP_DIR") {
        Ok(v) => backup_dir = v,
        Err(e) => {eprintln!("Error getting backup directory: {}", e); return;}
    }

    // Instantiate structs
    let fox = Fox::new(api_key);
    let nordpool = NordPool::new();
    let smhi = SMHI::new(LAT, LONG);

    //let _ = retry!(||fox.set_max_soc(&inverter_sn, 100)).unwrap();
    //return;

    // Create a first base schedule given only tariffs, charge level will later be updated
    let mut schedule: Schedule;
    match create_new_schedule(&nordpool, &smhi, None) {
        Ok(s) => schedule = s,
        Err(e) => {
            eprintln!("Error creating new schedule while starting up: {}", e);
            exit(1);
        }
    }
    for s in &schedule.blocks {
        println!("{}", s);
    }
    println!("================================================================================");

    // Check if we have an existing schedule for the day that then may be updated with
    // already started/running blocks
    match load_schedule(&backup_dir) {
        Ok(option) => {
            if let Some(s) = option {
                schedule = s;
            };
        },
        Err(_) => {}
    }
    for s in &schedule.blocks {
        println!("{}", s);
    }
    println!("================================================================================");


    // Main loop that runs once a minute
    let mut local_now: DateTime<Local>;
    let mut day_of_year = Local::now().ordinal0();
    loop {
        thread::sleep(Duration::from_secs(10));
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

        if let Some(b) = schedule.get_current_started_charge(local_now.hour() as u8) {
            if local_now.minute() % 5 == 0 {
                match set_full_if_done(&fox, &inverter_sn, schedule.blocks[b].max_soc) {
                    Ok(Some(status)) => {
                        schedule.update_block_status(b, status).unwrap();
                        save_schedule(&schedule, &backup_dir);
                    }
                    Err(e) => {
                        print_error(local_now, e, None);
                    }
                    _ => {}
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
                        print_error(local_now, e, Some(&block));
                    }

                },
                BlockType::Hold => {
                    if let Err(e) = set_hold(&fox, &inverter_sn, block.max_min_soc) {
                        print_error(local_now, e, Some(&block));
                    }
                },
                BlockType::Use => {
                    if let Err(e) = set_use(&fox, &inverter_sn) {
                        print_error(local_now, e, Some(&block));
                    }
                },
            }
            schedule.update_block_status(b, Status::Started).unwrap();

            // Save current schedule version
            save_schedule(&schedule, &backup_dir);
            for s in &schedule.blocks {
                println!("{}", s);
            }
            println!("================================================================================");
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
/// * 'sn' - serial number of the inverter
fn check_inverter_local_time(fox: &Fox, sn: &str) {
    let err: String;
    match retry!(||fox.get_device_time(sn)) {
        Ok(dt) => {
            let now = Local::now().naive_local();
            let delta = (now - dt).abs();

            if delta > chrono::Duration::minutes(1) {
                match fox.set_device_time(sn, now) {
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
/// * set the max soc which reflects how much room is needed for PV in following blocks
/// * set the charge schedule
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
/// * 'sn' - serial number of the inverter
/// * 'block' - the configuration to use
fn set_charge(fox: &Fox, sn: &str, block: &Block) -> Result<(), String> {
    let report_time = format!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    println!("{} - Setting charge block: maxSoC: {}, start: {}, end: {}",report_time, block.max_soc, block.start_hour, block.end_hour);

    let _ = retry!(||fox.set_max_soc(sn, block.max_soc))?;
    let _ = retry!(||fox.set_battery_charging_time_schedule(
                        sn,
                        true, block.start_hour, 0, block.end_hour, 59,
                        false, 0, 0, 0, 0,
                    ))?;

    Ok(())
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
/// * 'sn' - serial number of the inverter
/// * 'max_soc' - max soc
fn set_full_if_done(fox: &Fox, sn: &str, max_soc: u8) -> Result<Option<Status>, String> {
    let soc= retry!(||fox.get_current_soc(sn))?;
    if soc >= max_soc {
        let report_time = format!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
        println!("{} - Setting charge block to full",report_time);

        let _ = retry!(||fox.set_battery_charging_time_schedule(
                        sn,
                        false, 0, 0, 0, 0,
                        false, 0, 0, 0, 0,
                    ))?;

        let min_soc = max_soc.max(10).min(100);
        let _ = retry!(||fox.set_min_soc_on_grid(sn, min_soc))?;
        let _ = retry!(||fox.set_max_soc(sn, 100))?;

        Ok(Some(Status::Full))
    } else {
        Ok(None)
    }
}

/// Sets a hold block in the inverter
///
/// The logic for a hold block is a little busy since there is no equivalent in the inverter:
/// * disable any charge block just to make sure that it isn't surviving to the next day
/// * retrieve the current soc from the invert
/// * get the lowest of the two values max_min_soc and soc
///     * charge block may have exceeded it with PV power so soc is too high, in which we use max min soc
///     * charge block may have not fully reached max soc, in which case we use current soc
/// * make sure that we are within global limits, i.e. 10-100
/// * set the min soc on grid in the inverter
/// * set max soc to 100% in the inverter, we don't want to limit anything from PV
///
/// # Arguments
///
/// * 'fox' - reference to the Fox struct
/// * 'sn' - serial number of the inverter
/// * 'max_min_soc' - max min soc allowed for the block
fn set_hold(fox: &Fox, sn: &str, max_min_soc: u8) -> Result<(), String> {
    let report_time = format!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    println!("{} - Setting hold block",report_time);

    let _ = retry!(||fox.set_battery_charging_time_schedule(
                        sn,
                        false, 0, 0, 0, 0,
                        false, 0, 0, 0, 0,
                    ))?;

    let soc = retry!(||fox.get_current_soc(sn))?;
    let min_soc = max_min_soc.min(soc).max(10).min(100);
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
    let report_time = format!("{}", Local::now().format("%Y-%m-%d %H:%M:%S"));
    println!("{} - Setting use block",report_time);

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
    let tariffs = retry!(||nordpool.get_tariffs(Local::now().add(TimeDelta::days(d))))?;
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

/// Saves schedule to file as json
/// The filename gives at most one unique file per hour
///
/// # Arguments
///
/// * 'schedule' - the schedule to save
/// * 'backup_dir' - the directory to save the file to
fn save_schedule(schedule: &Schedule, backup_dir: &str) {
    let err: String;
    let file_path = format!("{}{}.json", backup_dir, Local::now().format("%Y%m%d_%H"));
    match serde_json::to_string(&schedule) {
        Ok(json) => {
            match fs::write(file_path, json) {
                Ok(_) => { return },
                Err(e) => { err = e.to_string() }
            }
        },
        Err(e) => { err = e.to_string() }
    }
    eprintln!("Error writing schedule to file: {}", err);
    eprintln!("This is recoverable")
}

/// Loads schedule from json on file
///
/// This is mostly to avoid re-executing an already started block, hence it will just find
/// the latest saved schedule for the current day
///
/// # Arguments
///
/// * 'backup_dir' - the directory to save the file to
fn load_schedule(backup_dir: &str) -> Result<Option<Schedule>, String> {
    let mut entries: Vec<String> = Vec::new();
    let file_path = format!("{}{}.json", backup_dir, Local::now().format("%Y%m%d*"));
    for entry in glob(&file_path)
        .map_err(|e| format!("Error searching directory: {}", e.to_string()))? {
        match entry {
            Ok(path) => {
                if path.is_file() {
                    if let Some(os_path) = path.to_str() {
                        entries.push(os_path.to_string());
                    }
                }
            },
            Err(e) => {
                return Err(format!("Error reading directory entry: {}", e.to_string()));
            }
        }
    }

    entries.sort();

    if entries.len() > 0 {
        match File::open(&entries[entries.len() - 1]) {
            Ok(mut file) => {
                let mut contents = String::new();
                match file.read_to_string(&mut contents).map_err(|e| e.to_string()) {
                    Ok(_) => {
                        let schedule: Schedule = serde_json::from_str(&contents)
                            .map_err(|e| format!("Error while parsing json to Schedule: {}", e.to_string()))?;
                        Ok(Some(schedule))
                    },
                    Err(e) => { Err(format!("Error while reading backup file: {}", e.to_string())) }
                }
            },
            Err(e) => { Err(format!("Error while open schedule file: {}", e.to_string())) }
        }
    } else {
        Ok(None)
    }
}
