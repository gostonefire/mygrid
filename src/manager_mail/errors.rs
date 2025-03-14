use std::fmt::{Display, Formatter};
use ureq::Error;


pub enum MailError {
    InvalidEmailAddress(String),
    Document(String),
    SendgridError(String),
}

impl Display for MailError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MailError::InvalidEmailAddress(e) => write!(f, "MailError::InvalidEmailAddress: {}", e),
            MailError::Document(e) => write!(f, "MailError::Document: {}", e),
            MailError::SendgridError(e) => write!(f, "MailError::SendgridError: {}", e),
        }
    }
}
impl From<serde_json::Error> for MailError {
    fn from(e: serde_json::Error) -> Self { MailError::Document(e.to_string()) }
}
impl From<Error> for MailError {
    fn from(e: Error) -> Self { MailError::SendgridError(e.to_string()) }
}
