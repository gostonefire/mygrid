use std::fmt::{Display, Formatter};
use std::time::Duration;
use ureq::{Agent, Error};
use crate::models::sendgrid::{Address, Content, Email, Personalizations};

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
pub struct Mail {
    api_key: String,
    agent: Agent,
    from: Address,
    to: Address,
}

impl Mail {
    /// Returns a new instance of the Mail struct
    ///
    /// # Arguments
    ///
    /// * 'api_key' - the api key for sendgrid
    /// * 'from' - sender email address
    /// * 'to' - receiver email address
    pub fn new(api_key: String, from: String, to: String) -> Result<Self, MailError> {
        let config = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build();

        let agent = config.into();

        Ok(
            Self {
                agent,
                api_key,
                from: from.parse::<Address>()?,
                to: to.parse::<Address>()?,
            }
        )
    }

    /// Sends a mail with the given subject and body
    ///
    /// # Arguments
    ///
    /// * 'subject' - the subject of the mail
    /// * 'body' - the body of the mail
    pub fn send_mail(&self, subject: String, body: String) -> Result<(), MailError> {

        let req = Email {
            personalizations: vec![Personalizations { to: vec![self.to.clone()]}],
            from: self.from.clone(),
            subject,
            content: vec![Content { content_type: "text/plain".to_string(), value: body }],
        };

        let json = serde_json::to_string(&req)?;

        let _ = self.agent
            .post("https://api.sendgrid.com/v3/mail/send")
            .content_type("application/json")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send(json)?;

        Ok(())
    }
}