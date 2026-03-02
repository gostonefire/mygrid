use foxess::FoxError;
use thiserror::Error;
use crate::manager_mail::errors::MailError;


/// Error depicting errors that occur during initialization of the main program
///
#[derive(Error, Debug)]
#[error("error while initializing mygrid: {0}")]
pub struct MyGridInitError(pub String);

impl From<ConfigError> for MyGridInitError {
    fn from(e: ConfigError) -> Self { MyGridInitError(e.to_string()) }
}
impl From<LoggingError> for MyGridInitError {
    fn from(e: LoggingError) -> Self { MyGridInitError(e.to_string()) }
}
impl From<MailError> for MyGridInitError {
    fn from(e: MailError) -> Self { MyGridInitError(e.to_string()) }
}
impl From<std::sync::PoisonError<std::sync::RwLockReadGuard<'_, bool>>> for MyGridInitError {
    fn from(e: std::sync::PoisonError<std::sync::RwLockReadGuard<'_, bool>>) -> Self { MyGridInitError(e.to_string()) }
}
impl From<std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, bool>>> for MyGridInitError {
    fn from(e: std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, bool>>) -> Self { MyGridInitError(e.to_string()) }
}
impl From<std::io::Error> for MyGridInitError {
    fn from(e: std::io::Error) -> Self { MyGridInitError(e.to_string()) }
}
impl From<serde_json::Error> for MyGridInitError {
    fn from(e: serde_json::Error) -> Self { MyGridInitError(e.to_string()) }
}
impl From<std::env::VarError> for MyGridInitError {
    fn from(e: std::env::VarError) -> Self { MyGridInitError(e.to_string()) }
}
impl From<std::string::FromUtf8Error> for MyGridInitError {
    fn from(e: std::string::FromUtf8Error) -> Self { MyGridInitError(e.to_string()) }
}
impl From<FoxError> for MyGridInitError {
    fn from(e: FoxError) -> Self { MyGridInitError(e.to_string()) }
}
impl From<FileManagerError> for MyGridInitError {
    fn from(e: FileManagerError) -> Self { MyGridInitError(e.to_string()) }
}

#[derive(Error, Debug)]
#[error("error while running mode schedule: {0}")]
pub struct ModeWorkerError(pub String);

/// Error depicting errors that occur while running the main program
///
#[derive(Error, Debug)]
#[error("error while running manual schedule: {0}")]
pub struct ManualWorkerError(pub String);

impl From<SkipError> for ManualWorkerError {
    fn from(e: SkipError) -> Self { ManualWorkerError(e.to_string()) }
}
impl From<&str> for ManualWorkerError {
    fn from(e: &str) -> Self { ManualWorkerError(e.to_string())}
}
impl From<SchedulingError> for ManualWorkerError {
    fn from(e: SchedulingError) -> Self { ManualWorkerError(e.to_string()) }
}
impl From<FoxError> for ManualWorkerError {
    fn from(e: FoxError) -> Self { ManualWorkerError(e.to_string()) }
}
impl From<std::sync::PoisonError<std::sync::RwLockReadGuard<'_, bool>>> for ManualWorkerError {
    fn from(e: std::sync::PoisonError<std::sync::RwLockReadGuard<'_, bool>>) -> Self { ManualWorkerError(e.to_string()) }
}
impl From<serde_json::Error> for ManualWorkerError {
    fn from(e: serde_json::Error) -> Self {
        ManualWorkerError(e.to_string())
    }
}
impl From<std::io::Error> for ManualWorkerError {
    fn from(e: std::io::Error) -> Self { ManualWorkerError(e.to_string()) }
}
impl From<FileManagerError> for ManualWorkerError {
    fn from(e: FileManagerError) -> Self { ManualWorkerError(e.to_string()) }
}

/// Error depicting errors that occur while doing config operations
///
#[derive(Error, Debug)]
#[error("error while loading configuration: {0}")]
pub struct ConfigError(String);

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError(e.to_string())
    }
}
impl From<toml::de::Error> for ConfigError {
    fn from(e: toml::de::Error) -> Self { ConfigError(e.to_string()) }
}

/// Error depicting errors that occur while setting up logging
///
#[derive(Error, Debug)]
#[error("error while setting up logging: {0}")]
pub struct LoggingError(String);

impl From<log4rs::config::runtime::ConfigErrors> for LoggingError {
    fn from(e: log4rs::config::runtime::ConfigErrors) -> Self { LoggingError(e.to_string()) }
}
impl From<log::SetLoggerError> for LoggingError {
    fn from(e: log::SetLoggerError) -> Self { LoggingError(e.to_string()) }
}
impl From<std::io::Error> for LoggingError {
    fn from(e: std::io::Error) -> Self { LoggingError(e.to_string()) }
}

/// Error depicting errors that occur while creating and managing schedules
///
#[derive(Error, Debug)]
#[error("error while managing/loading schedules: {0}")]
pub struct SchedulingError(pub String);

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

/// Error depicting errors that occur while doing skip file operations
///
#[derive(Error, Debug)]
#[error("error while managing manual days: {0}")]
pub struct SkipError(String);

impl From<std::io::Error> for SkipError {
    fn from(e: std::io::Error) -> Self {
        SkipError(e.to_string())
    }
}
impl From<serde_json::Error> for SkipError {
    fn from(e: serde_json::Error) -> Self {
        SkipError(e.to_string())
    }
}
impl From<std::sync::PoisonError<std::sync::RwLockReadGuard<'_, bool>>> for SkipError {
    fn from(e: std::sync::PoisonError<std::sync::RwLockReadGuard<'_, bool>>) -> Self { SkipError(e.to_string()) }
}
impl From<std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, bool>>> for SkipError {
    fn from(e: std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, bool>>) -> Self { SkipError(e.to_string()) }
}

#[derive(Error, Debug)]
#[error("error while managing file operations: {0}")]
pub struct FileManagerError(String);

impl From<&str> for FileManagerError {
    fn from(e: &str) -> Self {
        FileManagerError(e.to_string())
    }   
}
impl From<std::io::Error> for FileManagerError {
    fn from(e: std::io::Error) -> Self {
        FileManagerError(e.to_string())
    }
}

impl From<serde_json::Error> for FileManagerError {
    fn from(e: serde_json::Error) -> Self {
        FileManagerError(e.to_string())
    }
}

impl From<glob::PatternError> for FileManagerError {
    fn from(e: glob::PatternError) -> Self { FileManagerError(e.to_string()) }
}

impl From<chrono::format::ParseError> for FileManagerError {
    fn from(e: chrono::format::ParseError) -> Self { FileManagerError(e.to_string()) }
}

#[derive(Error, Debug)]
#[error("error while operating workers: {0}")]
pub struct WorkerError(String);

impl From<ManualWorkerError> for WorkerError {
    fn from(e: ManualWorkerError) -> Self { WorkerError(e.to_string()) }
}
impl From<ModeWorkerError> for WorkerError {
    fn from(e: ModeWorkerError) -> Self { WorkerError(e.to_string()) }
}

