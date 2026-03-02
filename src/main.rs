use std::sync::RwLock;
use std::thread;
use std::time::Duration;
use chrono::{DateTime, Local, TimeDelta, Utc};
use log::error;
use crate::config::Config;
use crate::errors::WorkerError;
use crate::initialization::{init, Mgr};
use crate::manager_mail::Mail;
use crate::manual_worker::run_manual_scheduler;
use crate::mode_worker::run_mode_scheduler;

mod macros;
mod initialization;
mod manual_worker;
mod errors;
mod manager_mail;
mod manual;
mod config;
mod logging;
mod manual_scheduler;
mod mode_scheduler;
mod manager_files;
mod mode_worker;

/// Debug mode means no write operations to inverter (except time)
static DEBUG_MODE: RwLock<bool> = RwLock::new(false);

/// Manual day mode means no write operations to inverter (except time)
static MANUAL_DAY: RwLock<bool> = RwLock::new(false);

static LOGGER_INITIALIZED: RwLock<bool> = RwLock::new(false);

fn main() {
    let mut n_errors = 0;
    let mut last_error = Utc::now();

    loop {
        let (config, mut mgr) = match init() {
            Ok((c, m)) => (c, m),
            Err(e) => {
                (n_errors, last_error) = manage_error(e.to_string(), n_errors, last_error, None);
                continue;
            }
        };

        match working_mode_switch(&config, &mut mgr) {
            Ok(()) => return,
            Err(e) => {
                error!("{}", e);
                (n_errors, last_error) = manage_error(e.to_string(), n_errors, last_error, Some(&mgr.mail));
            }
        }
    }
}

/// The work mode switch that given whatever work mode a schedule has been produced for
/// chooses the correct implementation. If no schedule has been produced, it defaults to manual mode.
///
/// Any worker that gracefully returns with a schedule is supposed to have decided that the schedule
/// is intended for the other worker. E.g. if the manual worker picks up the next available schedule,
/// and it is marked as for the mode scheduler, the schedule is returned to this switch for re-assignment.
///
/// # Arguments
///
/// * 'config' - The configuration for the workers
/// * 'mgr' - The manager for the workers
fn working_mode_switch(config: &Config, mut mgr: &mut Mgr) -> Result<(), WorkerError> {
    loop {
        let mode_scheduler = mgr.import_schedule
            .as_ref()
            .map(|s| s.mode_scheduler)
            .unwrap_or(false);
        
        if !mode_scheduler {
            mgr.import_schedule = run_manual_scheduler(config, &mut mgr)?;
        } else {
            mgr.import_schedule = run_mode_scheduler(config, &mut mgr)?;
        };
        
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
fn manage_error(msg: String, mut n_errors: i32, last_error: DateTime<Utc>, mail: Option<&Mail>) -> (i32, DateTime<Utc>) {
    if let Some(g) = mail {
        let _ = g.send_mail("Error caught".to_string(), msg.clone());
    }

    eprintln!("{}", msg);

    let now = Utc::now();
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
    (n_errors, Utc::now())
}

pub struct UtcNow {
    time_delta: TimeDelta,
}

impl UtcNow {
    /// Creates a new UtcNow struct with any eventual time_delta
    /// 
    /// # Arguments
    /// 
    /// * 'debug_run_start' - time that reflects a point in time that the worker's clock should start at
    pub fn new(debug_run_start: Option<DateTime<Local>>) -> Self {
        let time_delta = if let Some(debug_run_start) = debug_run_start {
            Utc::now() - debug_run_start.with_timezone(&Utc)
        } else {
            TimeDelta::minutes(0)
        };
        
        Self {
            time_delta,
        }
    }

    /// Returns utc now with any configured time delta applied
    ///
    pub fn utc_now(&self) -> DateTime<Utc> {
        Utc::now() - self.time_delta
    }
}