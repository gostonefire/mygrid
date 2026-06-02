use std::ops::Add;
use chrono::{DateTime, DurationRound, Local, TimeDelta, Timelike, Utc};
use foxess::{ExtraParam, FoxWorkModes, Group, TimeSegmentsDataRequest};
use log::{error, warn};
use crate::manager_files::save_import_schedule;
use crate::worker_common::{Block, BlockType, ImportSchedule, Status, BLOCK_UNIT_SIZE};
use crate::manager_mail::Mail;
use crate::scheduler_common::SchedulingError;

pub struct Schedule {
    pub import_schedule: ImportSchedule,
}

impl Schedule {
    /// Creates a new instance of a Schedule
    ///
    /// # Arguments
    ///
    /// * 'import_schedule' - The import schedule to use for the schedule
    pub fn new(import_schedule: ImportSchedule) -> Self {
        Self {
            import_schedule,
        }
    }

    /// Creates a new instance of an ImportSchedule
    ///
    pub fn new_default_import_schedule() -> ImportSchedule {
        let (start_time, end_time) = get_utc_day_start(Utc::now(), 0);

            ImportSchedule {
                blocks: vec![Block {
                    block_id: 0,
                    block_type: BlockType::Use,
                    start_time,
                    end_time: end_time.add(-TimeDelta::minutes(BLOCK_UNIT_SIZE)),
                    cost: 0.0,
                    true_soc_in: None,
                    soc_in: 10,
                    soc_out: 10,
                    status: Status::Waiting,
                }],
                schedule_id: 0,
            }
    }

    /// Creates a time segments data request struct and validates it
    ///
    /// # Arguments
    ///
    /// * 'mail' - Mail instance to send error messages
    pub fn create_schedule(&self, mail: &Mail) -> TimeSegmentsDataRequest {
        let mut groups: Vec<Group> = self.import_schedule.blocks
            .iter()
            .map(|b| {
                let local_start = b.start_time.with_timezone(&Local);
                let local_end = b.end_time.add(TimeDelta::minutes(BLOCK_UNIT_SIZE - 1)).with_timezone(&Local);
                Group {
                    start_hour: local_start.hour() as i64,
                    start_minute: local_start.minute() as i64,
                    end_hour: local_end.hour() as i64,
                    end_minute: local_end.minute() as i64,
                    work_mode: block_type_to_work_mode(&b.block_type),
                    extra_param: if b.block_type == BlockType::Charge {
                        Some(ExtraParam {
                            fd_pwr: None,
                            min_soc_on_grid: None,
                            fd_soc: Some(b.soc_out as f64),
                            max_soc: Some(b.soc_out as f64),
                            import_limit: None,
                            export_limit: None,
                            pv_limit: None,
                            reactive_power: None,
                        })
                    } else {
                        None
                    },
                }
            }).collect::<Vec<_>>();

        if let Some(first_block) = groups.first() {
            if !(first_block.start_hour == 0 && first_block.start_minute == 0) {
                groups.insert(0, Group {
                    start_hour: 0,
                    start_minute: 0,
                    end_hour: if first_block.start_minute == 0 { first_block.start_hour - 1} else { first_block.start_hour },
                    end_minute: if first_block.start_minute == 0 { 59 } else { first_block.start_minute - 1 },
                    work_mode: FoxWorkModes::SelfUse,
                    extra_param: None,
                });
            }
        };

        if let Err(e) = validate_schedule(&groups) {
            groups = vec![Group {
                start_hour: 0,
                start_minute: 0,
                end_hour: 23,
                end_minute: 59,
                work_mode: FoxWorkModes::SelfUse,
                extra_param: None,
            }];
            warn!("Error in imported schedule: {}\n\nUsing default schedule for mode scheduler", e.to_string());
            let _ = mail.send_mail("Mode Scheduler Error".to_string(), format!("Error in imported schedule: {}\n\nUsing default schedule", e.to_string()));
        }

        TimeSegmentsDataRequest {
            is_default: None,
            groups,
        }
    }

    /// Updates the schedule with a new status if the datetime and work mode are found in it
    ///
    /// # Arguments
    ///
    /// * 'schedule_dir' - the directory to save the file to
    /// * 'date_time' - the segment in time to update
    /// * 'work_mode' - the work mode the segment should be in
    /// * 'status' - the status to set the segment to
    /// * 'soc' - the soc to set the segment to
    pub fn update_import_schedule(&mut self, schedule_dir: &str, date_time: DateTime<Utc>, work_mode: FoxWorkModes, status: Status, soc: u8) -> Result<(), SchedulingError>{
        let block_type = work_mode_to_block_type(&work_mode);

        let block = self.import_schedule.blocks.iter_mut().filter(|b| {
            date_time >= b.start_time && date_time < b.end_time.add(TimeDelta::minutes(BLOCK_UNIT_SIZE))
        }).last();

        if let Some(b) = block && b.block_type == block_type && b.status != status {
            b.status = status;
            b.true_soc_in = Some(soc as usize);
            save_import_schedule(schedule_dir, &self.import_schedule)?;
        }

        Ok(())
    }
    
    /// Gets the current schedule status and work mode in the import_schedule for a given date and time
    /// 
    /// # Arguments
    ///
    /// * 'mail' - Mail instance to send error messages
    /// * 'date_time' - Date and time to check schedule status
    pub fn get_current_schedule_status(&self, mail: &Mail, date_time: DateTime<Utc>) -> Option<(Status, FoxWorkModes)> {
        let block = self.import_schedule.blocks.iter().filter(|b| {
            date_time >= b.start_time && date_time < b.end_time.add(TimeDelta::minutes(BLOCK_UNIT_SIZE))
        }).last();
        
        if let Some(b) = block {
            Some((b.status.clone(), block_type_to_work_mode(&b.block_type)))
        } else {
            error!("no block found for time {} when checking import schedule status", date_time);
            let _ = mail.send_mail("Mode Scheduler Error".to_string(), format!("No block found for time {} when checking import schedule status", date_time));
            None
        }
    }
}

/// Validates time segments
///
/// # Arguments
///
/// * 'ts_groups' - Time segments to validate
fn validate_schedule(ts_groups: &Vec<Group>) -> Result<(), SchedulingError> {
    let mut hour: i64 = 0;
    let mut minute: i64 = 0;
    
    if ts_groups.is_empty() {
        return Err(SchedulingError::Validation("No time segments found".to_string()));
    }
    
    let last_segment = ts_groups.len() - 1;
    
    for (i, b) in ts_groups.iter().enumerate() {
        if b.start_hour != hour || b.start_minute != minute {
            return Err(SchedulingError::Validation(format!("Time segment {} invalid start: {}:{} is {}:{}", i, hour, minute, b.start_hour, b.start_minute)));
        }
        hour = if b.end_minute == 59 { b.end_hour + 1 } else { b.end_hour };
        minute = if b.end_minute == 59 { 0 } else { b.end_minute + 1 };
        
        if i == last_segment && (b.end_hour != 23 || b.end_minute != 59) {
            return Err(SchedulingError::Validation(format!("Time segment {} invalid end: 23:59 is {}:{}", i, b.end_hour, b.end_minute)));
        }
    }
    
    Ok(())
}

/// Returns the start and end (non-inclusive) of a day in UTC time.
/// For DST switch days (summer to winter time and vice versa), the length of the day
/// will be either 23 hours (in the spring) or 25 hours (in the autumn).
///
/// # Arguments
///
/// * 'date_time' - date time to get utc day start and end for (in relation to Local timezone)
/// * 'day_index' - 0-based index of the day, 0 is today, -1 is yesterday, etc.
fn get_utc_day_start(date_time: DateTime<Utc>, day_index: i64) -> (DateTime<Utc>, DateTime<Utc>) {
    // First, go local and move hour to a safe place regarding DST day shift between summer and winter time.
    // Also, apply the day index to get to the desired day.
    let date = date_time.with_timezone(&Local).with_hour(12).unwrap().add(TimeDelta::days(day_index));

    // Then trunc to a whole hour and move time to the start of day local (Chrono manages offset change if necessary)
    let start = date.duration_trunc(TimeDelta::hours(1)).unwrap().with_hour(0).unwrap();

    // Then add one day and do the same as for start
    let end = date.add(TimeDelta::days(1)).duration_trunc(TimeDelta::hours(1)).unwrap().with_hour(0).unwrap();

    (start.with_timezone(&Utc), end.with_timezone(&Utc))
}

/// Translates between mygrid_scheduler work modes to FoxESS mode scheduler work modes
///
/// # Arguments
///
/// * 'block_type' - work mode to translate
fn block_type_to_work_mode(block_type: &BlockType) -> FoxWorkModes {
    match block_type {
        BlockType::Charge => FoxWorkModes::ForceCharge,
        BlockType::Hold => FoxWorkModes::Backup,
        BlockType::Use => FoxWorkModes::SelfUse,
        BlockType::Unknown => FoxWorkModes::Unknown,
    }
}

fn work_mode_to_block_type(work_mode: &FoxWorkModes) -> BlockType {
    match work_mode {
        FoxWorkModes::ForceCharge => BlockType::Charge,
        FoxWorkModes::Backup => BlockType::Hold,
        FoxWorkModes::SelfUse => BlockType::Use,
        _ => BlockType::Unknown,
    }
}