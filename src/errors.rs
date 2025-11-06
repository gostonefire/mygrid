use thiserror::Error;
use crate::manager_fox_cloud::errors::FoxError;
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

/// Error depicting errors that occur while running the main program
///
#[derive(Error, Debug)]
#[error("error while running schedule: {0}")]
pub struct MyGridWorkerError(pub String);

impl From<SkipError> for MyGridWorkerError {
    fn from(e: SkipError) -> Self { MyGridWorkerError(e.to_string()) }
}
impl From<&str> for MyGridWorkerError {
    fn from(e: &str) -> Self { MyGridWorkerError(e.to_string())}
}
impl From<SchedulingError> for MyGridWorkerError {
    fn from(e: SchedulingError) -> Self { MyGridWorkerError(e.to_string()) }
}
impl From<FoxError> for MyGridWorkerError {
    fn from(e: FoxError) -> Self { MyGridWorkerError(e.to_string()) }
}
impl From<std::sync::PoisonError<std::sync::RwLockReadGuard<'_, bool>>> for MyGridWorkerError {
    fn from(e: std::sync::PoisonError<std::sync::RwLockReadGuard<'_, bool>>) -> Self { MyGridWorkerError(e.to_string()) }
}
impl From<serde_json::Error> for MyGridWorkerError {
    fn from(e: serde_json::Error) -> Self {
        MyGridWorkerError(e.to_string())
    }
}
impl From<std::io::Error> for MyGridWorkerError {
    fn from(e: std::io::Error) -> Self { MyGridWorkerError(e.to_string()) }
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
