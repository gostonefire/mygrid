use foxess::{Fox, TimeSegmentsDataRequest};
use log::info;
use anyhow::Result;
use crate::retry;
use crate::config::Config;
use crate::worker_common::{import_schedule, is_manual_debug, WorkerError, Status};
use crate::initialization::Mgr;
use crate::mode_scheduler::create_schedule;

pub fn run_mode_scheduler(config: &Config, mgr: &mut Mgr) -> Result<(), WorkerError> {
    let mut schedule_id: i64 = 0;

    loop {
        let import_schedule = import_schedule(&config.files.schedule_dir, mgr.time.utc_now(), true)?
            .ok_or(WorkerError::Other("got None from import_schedule in mode_worker, should not be possible"))?;

        if import_schedule.schedule_id != schedule_id {
            schedule_id = import_schedule.schedule_id;

            let schedule = create_schedule(&mgr.mail, &import_schedule);
            let status = set_mode_schedule(&mgr.fox, &schedule)?;

        }


    }
}

fn set_mode_schedule(fox: &Fox, schedule: &TimeSegmentsDataRequest) -> Result<Status, WorkerError> {
    info!("setting charge block");
    if is_manual_debug()? {return Ok(Status::Started)}

    let _ = retry!(
        "set_scheduler_time_segments",
        || fox.set_scheduler_time_segments(schedule)
    )?;

    Ok(Status::Started)
}
