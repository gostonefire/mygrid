use std::fs;
use std::path::Path;
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
    pub summer_solstice: (u32, u32),
    pub winter_solstice: (u32, u32),
    pub sunrise_angle: f64,
    pub sunset_angle: f64,
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
    pub api_key: String,
    pub from: String,
    pub to: String,
}

#[derive(Deserialize)]
pub struct Files {
    pub backup_dir: String,
    pub stats_dir: String,
    pub manual_file: String,
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
}

#[derive(Deserialize)]
struct PVDiagram {
    pv_data: Vec<f64>,
}

#[derive(Deserialize)]
pub struct ConsumptionDiagram {
    pub day: [[f64; 24];7],
}

/// Loads the configuration file and returns a struct with all configuration items
/// 
/// # Arguments
/// 
/// * 'config_dir' - directory where to find configuration file
pub fn load_config(config_dir: &str) -> Result<Config, ConfigError> {
    let file_path = format!("{}config.toml", config_dir);
    
    let toml = fs::read_to_string(file_path)?;
    let mut config: Config = toml::from_str(&toml)?;
    
    let pv_diagram = load_pv_diagram(config_dir)?;
    let cons_diagram = load_consumption_diagram(config_dir)?;
    
    config.production.diagram = Some(pv_diagram);
    config.consumption.diagram = Some(cons_diagram);
    
    Ok(config)
}

/// Loads PV Diagram data
///
/// # Arguments
///
/// * 'config_dir' - the directory where to find config files
fn load_pv_diagram(config_dir: &str) -> Result<[f64;1440], ConfigError> {
    let file_path = format!("{}pv_diagram.json", config_dir);

    let path = Path::new(&file_path);
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
/// * 'config_dir' - the directory where to find config files
fn load_consumption_diagram(config_dir: &str) -> Result<[[f64;24];7], ConfigError> {
    let file_path = format!("{}consumption_diagram.json", config_dir);

    let path = Path::new(&file_path);
    if path.exists() {
        let json = fs::read_to_string(path)?;
        let consumption_diagram: ConsumptionDiagram = serde_json::from_str(&json)?;

        Ok(consumption_diagram.day)
    } else {
        Err(ConfigError::from("consumption diagram file not found"))
    }
}