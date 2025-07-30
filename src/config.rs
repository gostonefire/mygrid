use std::fs;
use std::path::Path;
use log::LevelFilter;
use serde::Deserialize;
use crate::errors::ConfigError;

#[derive(Deserialize)]
pub struct GeoRef {
    pub lat: f64,
    pub long: f64,
}

#[derive(Deserialize)]
pub struct ConsumptionParameters {
    pub min_avg_load: f64,
    pub max_avg_load: f64,
    #[serde(skip)]
    pub diagram: Option<[[f64;24];7]>,
}

#[derive(Deserialize)]
pub struct ProductionParameters {
    pub min_pv_power: f64,
    pub max_pv_power: f64,
    pub cloud_impact_factor: f64,
    pub low_clouds_factor: f64,
    pub mid_clouds_factor: f64,
    pub high_clouds_factor: f64,
    pub summer_solstice: (u32, u32),
    pub winter_solstice: (u32, u32),
    pub sunrise_angle: f64,
    pub sunset_angle: f64,
    pub visibility_alt: f64,
    pub am_x1: f64,
    pub am_y1: f64,
    pub am_x2: f64,
    pub am_y2: f64,
    pub pm_x1: f64,
    pub pm_y1: f64,
    pub pm_x2: f64,
    pub pm_y2: f64,
    #[serde(skip)]
    pub diagram: Option<[f64;1440]>,
}

#[derive(Deserialize)]
pub struct ChargeParameters {
    pub bat_capacity: f64,
    pub bat_kwh: f64,
    pub soc_kwh: f64,
    pub charge_kwh_hour: f64,
    pub charge_efficiency: f64,
    pub discharge_efficiency: f64,
    pub sell_priority: f64,
}

#[derive(Deserialize)]
pub struct FoxESS {
    pub api_key: String,
    pub inverter_sn: String,
}

#[derive(Deserialize)]
pub struct MailParameters {
    pub smtp_user: String,
    pub smtp_password: String,
    pub smtp_endpoint: String,
    pub from: String,
    pub to: String,
}

#[derive(Deserialize)]
pub struct Files {
    pub backup_dir: String,
    pub stats_dir: String,
    pub manual_file: String,
    pub pv_diagram: String,
    pub cons_diagram: String,
}

#[derive(Deserialize)]
pub struct General {
    pub log_path: String,
    pub log_level: LevelFilter,
    pub log_to_stdout: bool,
    pub debug_mode: bool,
}

#[derive(Deserialize)]
pub struct Config {
    pub geo_ref: GeoRef,
    pub consumption: ConsumptionParameters,
    pub production: ProductionParameters,
    pub charge: ChargeParameters,
    pub fox_ess: FoxESS,
    pub mail: MailParameters,
    pub files: Files,
    pub general: General,
}

#[derive(Deserialize)]
struct PVDiagram {
    pv_data: Vec<f64>,
}

#[derive(Deserialize)]
struct DaysDiagram {
    monday: [f64; 24],
    tuesday: [f64; 24],
    wednesday: [f64; 24],
    thursday: [f64; 24],
    friday: [f64; 24],
    saturday: [f64; 24],
    sunday: [f64; 24],
}

#[derive(Deserialize)]
struct HouseHoldConsumption {
    consumption_diagram: DaysDiagram
}

/// Loads the configuration file and returns a struct with all configuration items
/// 
/// # Arguments
/// 
/// * 'config_path' - path to the configuration file
pub fn load_config(config_path: &str) -> Result<Config, ConfigError> {
    
    let toml = fs::read_to_string(config_path)?;
    let mut config: Config = toml::from_str(&toml)?;
    
    let pv_diagram = load_pv_diagram(&config.files.pv_diagram)?;
    let cons_diagram = load_consumption_diagram(&config.files.cons_diagram)?;
    
    config.production.diagram = Some(pv_diagram);
    config.consumption.diagram = Some(cons_diagram);
    
    Ok(config)
}

/// Loads PV Diagram data
///
/// # Arguments
///
/// * 'diagram_path' - path to the pv diagram file
fn load_pv_diagram(diagram_path: &str) -> Result<[f64;1440], ConfigError> {

    let path = Path::new(&diagram_path);
    if path.exists() {
        let mut result: [f64;1440] = [0.0;1440];

        let json = fs::read_to_string(path)?;
        let pv_diagram: PVDiagram = serde_json::from_str(&json)?;

        if pv_diagram.pv_data.len() != 1440 {
            return Err(ConfigError::from("PV diagram length mismatch"))
        }

        for (i, p) in pv_diagram.pv_data.iter().enumerate() {
            result[i] = *p;
        }

        Ok(result)
    } else {
        Err(ConfigError::from("PV diagram file not found"))
    }
}

/// Loads consumption diagram configuration
///
/// # Arguments
///
/// * 'diagram_path' - path to the consumption diagram file
fn load_consumption_diagram(diagram_path: &str) -> Result<[[f64;24];7], ConfigError> {
    
    let toml = fs::read_to_string(diagram_path)?;
    let hhc: HouseHoldConsumption = toml::from_str(&toml)?;
    
    let days: [[f64;24];7] = [
        hhc.consumption_diagram.monday,
        hhc.consumption_diagram.tuesday,
        hhc.consumption_diagram.wednesday,
        hhc.consumption_diagram.thursday,
        hhc.consumption_diagram.friday,
        hhc.consumption_diagram.saturday,
        hhc.consumption_diagram.sunday];

        Ok(days)
}