use chrono::{Local};
use serde::{Deserialize, Serialize};
use serde_with::chrono::DateTime;

#[derive(Deserialize, Debug)]
pub struct EntryPerArea {
    #[serde(rename = "SE4")]
    pub se4: f64,
}

#[derive(Deserialize, Debug)]
pub struct MultiAreaEntries {
    #[serde(rename = "deliveryStart")]
    pub delivery_start: DateTime<Local>,
    #[serde(rename = "entryPerArea")]
    pub entry_per_area: EntryPerArea,
}

#[derive(Deserialize, Debug)]
pub struct Tariffs {
    #[serde(rename = "multiIndexEntries")]
    pub multi_area_entries: Vec<MultiAreaEntries>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct TariffValues {
    pub valid_time: DateTime<Local>,
    pub price: f64,
    pub buy: f64,
    pub sell: f64,
}