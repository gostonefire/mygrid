use thiserror::Error;
use crate::manager_files::FileManagerError;

#[derive(Error, Debug)]
#[error("error while managing/loading schedules: {0}")]
pub enum SchedulingError {
    //#[error("error while reading/writing file: {0}")]
    //IO(#[from] std::io::Error),
    //#[error("error while parsing JSON: {0}")]
    //JSON(#[from] serde_json::Error),
    //#[error("error while parsing glob pattern: {0}")]
    //GLOB(#[from] glob::PatternError),
    //#[error("error while parsing date: {0}")]
    //DATE(#[from] chrono::format::ParseError),
    #[error(transparent)]
    FileManager(#[from] FileManagerError),
    #[error("validation error: {0}")]
    Validation(String),

}

/*
impl From<&str> for SchedulingError {
    fn from(e: &str) -> Self {
        SchedulingError(e.to_string())
    }
}

impl From<serde_json::Error> for SchedulingError {
    fn from(e: serde_json::Error) -> Self { SchedulingError(e.to_string()) }
}
impl From<glob::PatternError> for SchedulingError {
    fn from(e: glob::PatternError) -> Self { SchedulingError(e.to_string()) }
}
impl From<chrono::format::ParseError> for SchedulingError {
    fn from(e: chrono::format::ParseError) -> Self { SchedulingError(e.to_string()) }
}
impl From<FileManagerError> for SchedulingError {
    fn from(e: FileManagerError) -> Self { SchedulingError(e.to_string()) }
}


 */