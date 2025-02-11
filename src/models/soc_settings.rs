use serde::{Deserialize, Serialize};


#[derive(Deserialize)]
pub struct SocCurrentResult {
    pub result: Vec<SocCurrentVariables>,
}

#[derive(Deserialize)]
pub struct SocCurrentVariables {
    pub datas: Vec<SocCurrentData>,
    pub time: String,
    #[serde(rename = "deviceSN")]
    pub device_sn: String,
}

#[derive(Deserialize)]
pub struct SocCurrentData {
    pub unit: String,
    pub name: String,
    pub variable: String,
    pub value: f64,
}

#[derive(Serialize)]
pub struct RequestCurrentSoc {
    pub sn: String,
    pub variables: Vec<String>,
}

#[derive(Deserialize)]
pub struct SocSettingResult {
    pub result: SocSetting,
}

#[derive(Deserialize)]
pub struct SocSetting {
    pub unit: String,
    pub precision: f64,
    pub range: SocSettingRange,
    pub value: String,
}

#[derive(Deserialize)]
pub struct SocSettingRange {
    pub min: f64,
    pub max: f64,
}

#[derive(Serialize)]
pub struct RequestSoc {
    pub sn: String,
    pub key: String,
}

#[derive(Serialize)]
pub struct SetSoc {
    pub sn: String,
    pub key: String,
    pub value: String,
}
