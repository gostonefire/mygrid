use std::ops::Add;
use chrono::{Local, TimeDelta, Timelike};
use foxess::{ExtraParam, FoxWorkModes, Group, TimeSegmentsDataRequest};
use log::warn;
use crate::worker_common::{BlockType, ImportSchedule, BLOCK_UNIT_SIZE};
use crate::manager_mail::Mail;
use crate::scheduler_common::SchedulingError;

pub fn create_schedule(mail: &Mail, import_schedule: &ImportSchedule) -> TimeSegmentsDataRequest {
    let mut groups: Vec<Group> = import_schedule.blocks
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
                        max_soc: None,
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
        if first_block.start_hour != 0 && first_block.start_minute != 0 {
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
        groups.push(Group {
            start_hour: 0,
            start_minute: 0,
            end_hour: 23,
            end_minute: 59,
            work_mode: FoxWorkModes::SelfUse,
            extra_param: None,
        });
        warn!("Error in imported schedule: {}\n\nUsing default schedule for mode scheduler", e.to_string());
        let _ = mail.send_mail("Mode Scheduler Error".to_string(), format!("Error in imported schedule: {}\n\nUsing default schedule", e.to_string()));
    }
    
    TimeSegmentsDataRequest {
        is_default: None,
        groups,
    }
}

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
        
        if i != last_segment && b.end_hour != 23 || b.end_minute != 59 {
            return Err(SchedulingError::Validation(format!("Time segment {} invalid end: 23:59 is {}:{}", i, b.end_hour, b.end_minute)));
        }
    }
    
    Ok(())
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
    }
}
