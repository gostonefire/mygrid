use std::fmt;
use ureq::Error;


pub enum SMHIError {
    SMHI(String),
    Document(String),
}

impl fmt::Display for SMHIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SMHIError::SMHI(e) => write!(f, "SMHIError::SMHI: {}", e),
            SMHIError::Document(e) => write!(f, "SMHIError::Document: {}", e),
        }
    }
}
impl From<Error> for SMHIError {
    fn from(e: Error) -> Self {
        SMHIError::SMHI(e.to_string())
    }
}
impl From<serde_json::Error> for SMHIError {
    fn from(e: serde_json::Error) -> Self {
        SMHIError::Document(e.to_string())
    }
}
