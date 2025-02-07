use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct DeviceTimeResult {
    pub result: DeviceTime,
}

#[derive(Serialize, Deserialize)]
pub struct DeviceTime {
    pub year: String,
    pub month: String,
    pub day: String,
    pub hour: String,
    pub minute: String,
    pub second: String,
}