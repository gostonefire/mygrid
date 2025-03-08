use serde::{Deserialize, Serialize};
use serde_with::chrono::DateTime;
use chrono::Local;

#[derive(Deserialize)]
pub struct FullParameters {
    pub name: String,
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
    #[serde(rename = "timeSeries")]
    pub time_series: Vec<FullTimeSeries>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct TimeValues {
    pub valid_time: DateTime<Local>,
    pub temp: f64,
    pub cloud: f64,
}