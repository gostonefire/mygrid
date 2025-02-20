use serde::{Deserialize};

#[derive(Deserialize)]
pub struct DeviceDetailsResult {
    pub result: DeviceDetails,
}

#[derive(Deserialize)]
pub struct DeviceDetails {
    #[serde(rename = "deviceType")]
    pub device_type: String,
    #[serde(rename = "masterVersion")]
    pub master_version: String,
    #[serde(rename = "afciVersion")]
    pub afci_version: String,
    #[serde(rename = "hasPV")]
    pub has_pv: bool,
    #[serde(rename = "deviceSN")]
    pub device_sn: String,
    #[serde(rename = "slaveVersion")]
    pub slave_version: String,
    #[serde(rename = "hasBattery")]
    pub has_battery: bool,
    pub function: DeviceDetailsFunction,
    #[serde(rename = "hardwareVersion")]
    pub hardware_version: String,
    #[serde(rename = "managerVersion")]
    pub manager_version: String,
    #[serde(rename = "stationName")]
    pub station_name: String,
    #[serde(rename = "moduleSN")]
    pub module_sn: String,
    #[serde(rename = "productType")]
    pub product_type: String,
    #[serde(rename = "stationID")]
    pub station_id: String,
    pub status: u8,
}

#[derive(Deserialize)]
pub struct DeviceDetailsFunction {
    pub scheduler: bool,
}