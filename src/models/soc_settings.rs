use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct SocSettingsResult {
    pub result: SoCSettings,
}

#[derive(Serialize, Deserialize)]
pub struct SoCSettings {
    #[serde(rename = "minSocOnGrid")]
    pub min_soc_on_grid: u8,
    #[serde(rename = "minSoc")]
    pub min_soc: u8,
}


