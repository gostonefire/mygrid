use std::f64::consts::PI;
use chrono::{DateTime, Datelike, Local, Timelike};
use trig::Trig;

/// Calculates the declination given a medium exact algorithm as described
/// here: https://www.reuk.co.uk/wordpress/solar/solar-declination/
///
/// # Arguments
///
/// * 'date' - the local date time
pub fn get_declination(date: DateTime<Local>) -> f64 {
    let day = date.ordinal0() as f64;

    let earth_tilt = -23.44;
    let p1 = earth_tilt.sind();
    let p2 = 360.0 / 365.24 * (day + 10.0);
    let p3 = 360.0 / PI * 0.0167 * (360.0 / 365.24 * (day - 2.0)).sind();
    let declination = (p1 * (p2 + p3).cosd()).asind();

    declination
}

/// Calculates the sun elevation given the algorithm as described
/// here: https://www.pveducation.org/pvcdrom/properties-of-sunlight/elevation-angle
///
/// # Arguments
///
/// * 'date' - the local date time
/// * 'lat' - the latitude given in decimal format
/// * 'long' - the longitude given in decimal format
/// * 'declination' - the current sun declination
pub fn get_elevation(date: DateTime<Local>, lat: f64, long: f64, declination: f64) -> f64 {
    let lstm = 15.0 * (date.offset().local_minus_utc() / 3600) as f64;
    let b = 360.0 / 365.0 * (date.ordinal0() as f64 - 81.0);
    let eot = 9.87 * (2.0 * b).sind() - 7.53 * b.cosd() - 1.5 * b.sind();
    let tc = 4.0 * (long - lstm) + eot;
    let lst = date.hour() as f64 + date.minute() as f64 / 60.0 + tc / 60.0;
    let hra = 15.0 * (lst - 12.0);

    (declination.sind() * lat.sind() + declination.cosd() * lat.cosd() * hra.cosd()).asind()
}

