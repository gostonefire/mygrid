use std::sync::RwLock;
use std::thread;
use std::time::Duration;
use chrono::{DateTime, Local};
use crate::initialization::init;
use crate::manager_mail::Mail;
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
mod backup;
mod manager_mail;
mod manual;

/// Latitude of the power plant
const LAT: f64 = 56.22332313734338;

/// Longitude of the power plant
const LONG: f64 = 15.658393416666142;

/// Debug mode means no write operations to inverter (except time)
static DEBUG_MODE: RwLock<bool> = RwLock::new(false);

/// Manual day mode means no write operations to inverter (except time)
static MANUAL_DAY: RwLock<bool> = RwLock::new(false);

fn main() {
    let mut n_errors = 0;
    let mut last_error = Local::now();

    loop {
        let (fox, nordpool, mut smhi, schedule, gmail, backup_dir, stats_dir, manual_file) = match init() {
            Ok((f, n, s, sc, g, b, st, mf)) => (f, n, s, sc, g, b, st, mf),
            Err(e) => {
                (n_errors, last_error) = manage_error(e.to_string(), n_errors, last_error, None);
                continue;
            }
        };

        match run(fox, nordpool, &mut smhi, schedule, &gmail, backup_dir, stats_dir, manual_file) {
            Ok(()) => return,
            Err(e) => {
                (n_errors, last_error) = manage_error(e.to_string(), n_errors, last_error, Some(&gmail));
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
/// An email will be sent if GMail was passed, but since it may be the mail function itself
/// that is the culprit, no checking for success is done.
///
/// # Arguments
///
/// * 'msg' - the error message to print to std err
/// * 'n_errors' - the number of errors occurred so far with spacing under one hour
/// * 'last_error' - the time the last error occurred
/// * 'mail' - mail sender struct
fn manage_error(msg: String, mut n_errors: i32, last_error: DateTime<Local>, mail: Option<&Mail>) -> (i32, DateTime<Local>) {
    if let Some(g) = mail {
        let _ = g.send_mail("Error caught".to_string(), msg.clone());
    }

    eprintln!("{}", msg);

    let now = Local::now();
    if now - last_error > chrono::Duration::minutes(60) {
        n_errors = 1;
    } else if n_errors >= 10 {
        if let Some(m) = mail {
            let _ = m.send_mail("Error caught".to_string(), "Not resolved within time limit!\nWill panic!".to_string());
            thread::sleep(Duration::from_secs(10));
        }
        panic!();
    }
    n_errors += 1;

    thread::sleep(Duration::from_secs(600));
    (n_errors, Local::now())
}