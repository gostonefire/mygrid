use serde::{Deserialize, Serialize};
use serde_with::chrono::DateTime;
use chrono::{Local, Utc};

#[derive(Deserialize)]
pub struct Data {
    pub air_temperature: f64,
    pub low_type_cloud_area_fraction: f64,
    pub medium_type_cloud_area_fraction: f64,
    pub high_type_cloud_area_fraction: f64,
}


#[derive(Deserialize)]
pub struct FullTimeSeries {
    pub time: DateTime<Utc>,
    pub data: Data,
}


#[derive(Deserialize)]
pub struct FullForecast {
    #[serde(rename = "timeSeries")]
    pub time_series: Vec<FullTimeSeries>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct ForecastValues {
    pub valid_time: DateTime<Local>,
    pub temp: f64,
    pub lcc_mean: f64,
    pub mcc_mean: f64,
    pub hcc_mean: f64,
    pub cloud_factor: f64,
}