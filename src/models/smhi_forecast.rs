use serde::{Deserialize};
use serde_with::chrono::DateTime;
use chrono::Local;

#[derive(Deserialize)]
pub struct FullParameters {
    pub name: String,
    #[serde(rename = "levelType")]
    pub level_type: String,
    pub level: i64,
    pub unit: String,
    pub values: Vec<f64>,
}

#[derive(Deserialize)]
pub struct FullTimeSeries {
    #[serde(rename = "validTime")]
    pub valid_time: DateTime<Local>,
    pub parameters: Vec<FullParameters>,
}


#[derive(Deserialize)]
pub struct FullForecast {
    #[serde(rename = "approvedTime")]
    pub approved_time: DateTime<Local>,
    #[serde(rename = "referenceTime")]
    pub reference_time: DateTime<Local>,
    #[serde(rename = "timeSeries")]
    pub time_series: Vec<FullTimeSeries>,
}

#[derive(Debug, Copy, Clone)]
pub struct TimeValues {
    pub valid_time: DateTime<Local>,
    pub temp: f64,
    pub cloud: f64,
}