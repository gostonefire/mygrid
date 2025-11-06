use thiserror::Error;

#[derive(Error, Debug)]
#[error("error in communication with FoxESS cloud: {0}")]
pub struct FoxError(pub String);
impl From<serde_json::Error> for FoxError {
    fn from(e: serde_json::Error) -> FoxError {
        FoxError(format!("json document error: {}", e.to_string()))
    }
}
impl From<ureq::Error> for FoxError {
    fn from(e: ureq::Error) -> FoxError {
        FoxError(format!("http request error: {}", e.to_string()))
    }
}
