use crate::initialization::init;
use crate::worker::run;

mod manager_nordpool;
mod manager_fox_cloud;
mod manager_sun;
mod models;
mod manager_smhi;
mod scheduling;
mod production;
mod consumption;
mod macros;
mod initialization;
mod worker;

/// Latitude of the power plant
const LAT: f64 = 56.22332313734338;

/// Longitude of the power plant
const LONG: f64 = 15.658393416666142;

fn main() {
    let (fox, nordpool, smhi, schedule, backup_dir) = match init() {
        Ok((f, n, s, sc, b)) => (f, n, s, sc, b),
        Err(msg) => panic!("{}", msg)
    };

    match run(fox, nordpool, smhi, schedule, backup_dir) {
        Ok(()) => return,
        Err(msg) => panic!("{}", msg)
    }
}

