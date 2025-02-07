use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct ChargingTimeResult {
    pub result: ChargingTimeSchedule,
}

#[derive(Serialize, Deserialize)]
pub struct ChargingTimeSchedule {
    #[serde(rename = "enable1")]
    pub enable_1: bool,
    #[serde(rename = "startTime1")]
    pub start_time_1: ChargingTime,
    #[serde(rename = "endTime1")]
    pub end_time_1: ChargingTime,
    #[serde(rename = "enable2")]
    pub enable_2: bool,
    #[serde(rename = "startTime2")]
    pub start_time_2: ChargingTime,
    #[serde(rename = "endTime2")]
    pub end_time_2: ChargingTime,
}

#[derive(Serialize, Deserialize)]
pub struct ChargingTime {
    pub hour: u8,
    pub minute: u8,
}
