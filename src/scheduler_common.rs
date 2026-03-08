use thiserror::Error;
use crate::manager_files::FileManagerError;

#[derive(Error, Debug)]
#[error("error while managing/loading schedules: {0}")]
pub enum SchedulingError {
    #[error(transparent)]
    FileManager(#[from] FileManagerError),
    #[error("validation error: {0}")]
    Validation(String),
}
