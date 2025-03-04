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

fn main() {
    let mut n_errors = 0;
    let mut last_error = Local::now();

    loop {
        let (fox, nordpool, smhi, schedule, backup_dir) = match init() {
            Ok((f, n, s, sc, b)) => (f, n, s, sc, b),
            Err(e) => {
                (n_errors, last_error) = manage_error(e.to_string(), n_errors, last_error);
                continue;
            }
        };

        match run(fox, nordpool, smhi, schedule, backup_dir) {
            Ok(()) => return,
            Err(e) => {
                (n_errors, last_error) = manage_error(e.to_string(), n_errors, last_error);
            }
        }
    }
}

fn manage_error(msg: String, mut n_errors: i32, last_error: DateTime<Local>) -> (i32, DateTime<Local>) {
    eprintln!("{}", msg);

    if Local::now() - last_error > chrono::Duration::minutes(60) {
        n_errors = 1;
    } else if n_errors >= 10 {
        panic!();
    }
    n_errors += 1;

    (n_errors, Local::now())
}