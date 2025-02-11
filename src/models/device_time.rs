use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct DeviceTimeResult {
    pub result: DeviceTime,
}

#[derive(Serialize, Deserialize)]
pub struct DeviceTime {
    #[serde(skip_deserializing)]
    pub sn: String,
    pub year: String,
    pub month: String,
    pub day: String,
    pub hour: String,
    pub minute: String,
    pub second: String,
}

#[derive(Serialize)]
pub struct RequestTime {
    pub sn: String,
}