use std::thread;
use std::time::Duration;
use chrono::{DateTime, Local};
use crate::initialization::init;
use crate::worker::run;

mod manager_nordpool;
mod manager_fox_cloud;
mod manager_sun;
mod models;
mod manager_smhi;
mod scheduling;
mod production;
mod consumption;
mod macros;
mod initialization;
mod worker;
mod errors;

/// Latitude of the power plant
const LAT: f64 = 56.22332313734338;

/// Longitude of the power plant
const LONG: f64 = 15.658393416666142;

/// Debug mode means no write operations to inverter (except time)
static mut DEBUG_MODE: bool = false;

fn main() {
    let mut n_errors = 0;
    let mut last_error = Local::now();

    loop {
        let (fox, nordpool, mut smhi, schedule, backup_dir) = match init() {
            Ok((f, n, s, sc, b)) => (f, n, s, sc, b),
            Err(e) => {
                (n_errors, last_error) = manage_error(e.to_string(), n_errors, last_error);
                continue;
            }
        };

        match run(fox, nordpool, &mut smhi, schedule, backup_dir) {
            Ok(()) => return,
            Err(e) => {
                (n_errors, last_error) = manage_error(e.to_string(), n_errors, last_error);
            }
        }
    }
}

/// Manage top level errors
///
/// It prints the error to standard err and determines if we have had to many errors and thereby
/// need to panic out of the application.
///
/// Errors are counted as long as they occur at least within an hour from each other, otherwise
/// the counter is reset to last occurred error.
///
/// The function sleeps 10 minutes before releasing for a new try
///
/// # Arguments
///
/// * 'msg' - the error message to print to std err
/// * 'n_errors' - the number of errors occurred so far with spacing under one hour
/// * 'last_error' - the time the last error occurred
fn manage_error(msg: String, mut n_errors: i32, last_error: DateTime<Local>) -> (i32, DateTime<Local>) {
    eprintln!("{}", msg);

    let now = Local::now();
    if now - last_error > chrono::Duration::minutes(60) {
        n_errors = 1;
    } else if n_errors >= 10 {
        panic!();
    }
    n_errors += 1;

    thread::sleep(Duration::from_secs(600));
    (n_errors, Local::now())
}