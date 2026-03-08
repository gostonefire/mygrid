use std::thread;
use std::time::Duration as StdDuration;
use foxess::{Fox, FoxWorkModes, TimeSegmentsDataRequest};
use log::{error, info};
use anyhow::Result;
use chrono::Duration;
use foxess::fox_settings::WorkMode;
use crate::retry;
use crate::config::Config;
use crate::worker_common::{import_schedule, is_manual_debug, WorkerError, Status};
use crate::initialization::Mgr;
use crate::manager_mail::Mail;
use crate::mode_scheduler::Schedule;

pub fn run_mode_scheduler(config: &Config, mgr: &mut Mgr) -> Result<(), WorkerError> {
    info!("running mode scheduler");

    let mut instant = mgr.time.utc_now();
    let mut schedule: Option<Schedule> = None;

    loop {
        thread::sleep(StdDuration::from_secs(10));

        let import_schedule = import_schedule(&config.files.schedule_dir, mgr.time.utc_now(), true)?
            .ok_or(WorkerError::Other("got None from import_schedule in mode_worker, should not be possible"))?;

        if schedule.is_none() || import_schedule.schedule_id != schedule.as_ref().unwrap().import_schedule.schedule_id {
            schedule = Some(Schedule::new(import_schedule));

            let time_segments = schedule.as_ref().unwrap().create_schedule(&mgr.mail);

            set_mode_schedule(&mgr.fox, &time_segments)?;
            check_switch_status(&mgr.fox, &mgr.mail)?;
        }

        if mgr.time.utc_now() - instant > Duration::seconds(60) && let Some(s) = schedule.as_mut() {
            instant = mgr.time.utc_now();

            if let Some(status) = s.get_current_schedule_status(&mgr.mail, instant) {
                if status == Status::Waiting {
                    let work_mode = get_working_mode(&mgr.fox)?;
                    s.update_import_schedule(&config.files.schedule_dir, instant, work_mode, Status::Started)?;
                }
            } 
        }
    }
}

/// Checks if the inverter is in Mode Scheduler mode
///
/// The current functionality only checks and reports as an error through logs and mail.
///
/// # Arguments
///
/// * 'fox' - the foxess client to use
/// * 'mail' - the mail client to use
fn check_switch_status(fox: &Fox, mail: &Mail) -> Result<(), WorkerError> {
    info!("checking switch status");
    if is_manual_debug()? {return Ok(())}

    let x = retry!(
        "fox.get_main_switch_status",
        || fox.get_main_switch_status(),
    )?;

    if !x.enable {
        error!("Inverter switch status indicates inverter is not in Mode Scheduler mode");
        let _ = mail.send_mail("Mode Scheduler Error".to_string(), "Inverter switch status indicates inverter is not in Mode Scheduler mode".to_string());
    }

    Ok(())
}


/// Get the current work mode from FoxESS
///
/// # Arguments
///
/// * 'fox' - the foxess client to use
fn get_working_mode(fox: &Fox) -> Result<FoxWorkModes, WorkerError> {
    info!("getting working mode");
    if is_manual_debug()? {return Ok(FoxWorkModes::SelfUse)}

    let wm = retry!(
        "get_setting_typed::<WorkMode>",
        || fox.get_setting_typed::<WorkMode>(),
    )?;

    Ok(wm)
}

/// Set mode scheduler schedule
///
/// # Arguments
///
/// * 'fox' - the foxess client to use
/// * 'schedule' - the schedule to set
fn set_mode_schedule(fox: &Fox, schedule: &TimeSegmentsDataRequest) -> Result<(), WorkerError> {
    info!("setting mode scheduler schedule");
    if is_manual_debug()? {return Ok(())}

    let _ = retry!(
        "set_scheduler_time_segments",
        || fox.set_scheduler_time_segments(schedule),
    )?;

    Ok(())
}
