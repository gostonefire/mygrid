use thiserror::Error;

#[derive(Error, Debug)]
#[error("error in communication with mail provider cloud: {0}")]
pub enum MailError {
    #[error("error in address: {0}")]
    Address(#[from] lettre::address::AddressError),
    #[error("error in transport: {0}")]
    Transport(#[from] lettre::transport::smtp::Error),
    #[error("error in message builder: {0}")]
    MessageBuilder(#[from] lettre::error::Error),
}
