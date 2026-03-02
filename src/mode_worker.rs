use crate::config::Config;
use crate::errors::ModeWorkerError;
use crate::initialization::Mgr;
use crate::manual_scheduler::ImportSchedule;

pub fn run_mode_scheduler(config: &Config, mgr: &mut Mgr) -> Result<Option<ImportSchedule>, ModeWorkerError> {

    Ok(None)
}