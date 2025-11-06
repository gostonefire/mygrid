use thiserror::Error;

#[derive(Error, Debug)]
#[error("error in communication with mail provider cloud: {0}")]
pub struct MailError(pub String);

impl From<lettre::transport::smtp::Error> for MailError {
    fn from(e: lettre::transport::smtp::Error) -> Self { MailError(e.to_string()) }
}
impl From<lettre::address::AddressError> for MailError {
    fn from(e: lettre::address::AddressError) -> Self { MailError(e.to_string()) }
}
impl From<lettre::error::Error> for MailError {
    fn from(e: lettre::error::Error) -> Self { MailError(e.to_string()) }
}