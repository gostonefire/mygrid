use serde::{Deserialize, Serialize};
use serde_with::chrono::DateTime;
use chrono::Local;

#[derive(Deserialize)]
pub struct ForecastRecord {
    pub date_time: DateTime<Local>,
    pub temperature: f64,
    pub lcc_mean: u8,
    pub mcc_mean: u8,
    pub hcc_mean: u8,
}

#[derive(Serialize)]
pub struct ForecastValues {
    pub valid_time: DateTime<Local>,
    pub temp: f64,
    pub lcc_mean: f64,
    pub mcc_mean: f64,
    pub hcc_mean: f64,
    pub cloud_factor: f64,
}
