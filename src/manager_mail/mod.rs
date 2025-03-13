use std::fmt::{Display, Formatter};
use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use lettre::address::AddressError;
use lettre::message::Mailbox;


const SMTP_RELAY: &str = "smtp.gmail.com";

pub struct GMailError(pub String);

impl Display for GMailError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result { write!(f, "GMailError: {}", self.0) }
}
impl From<AddressError> for GMailError {
    fn from(e: AddressError) -> Self { GMailError(e.to_string()) }
}
impl From<lettre::error::Error> for GMailError {
    fn from(e: lettre::error::Error) -> Self { GMailError(e.to_string()) }
}
impl From<lettre::transport::smtp::Error> for GMailError {
    fn from(e: lettre::transport::smtp::Error) -> Self { GMailError(e.to_string()) }
}

pub struct GMail {
    creds: Credentials,
    from: Mailbox,
    to: Mailbox,
}

impl GMail {
    /// Returns a new instance of the GMail struct
    ///
    /// # Arguments
    ///
    /// * 'login' - login name for gmail
    /// * 'password' - app password for the mygrid google app
    /// * 'from' - sender email address
    /// * 'to' - receiver email address
    pub fn new(login: String, password: String, from: String, to: String) -> Result<GMail, GMailError> {
        Ok(
            GMail {
                creds: Credentials::new(login, password),
                from: from.parse::<Mailbox>()?,
                to: to.parse::<Mailbox>()?,
            }
        )
    }

    /// Sends a mail with the given subject and body
    ///
    /// # Arguments
    ///
    /// * 'subject' - the subject of the mail
    /// * 'body' - the body of the mail
    pub fn send_mail(&self, subject: String, body: String) -> Result<(), GMailError> {
        let email = Message::builder()
            .from(self.from.clone())
            .reply_to(self.from.clone())
            .to(self.to.clone())
            .subject(subject)
            .header(ContentType::TEXT_PLAIN)
            .body(body)?;

        let mailer = SmtpTransport::relay(SMTP_RELAY)?
            .credentials(self.creds.clone())
            .build();

        mailer.send(&email)?;

        Ok(())
    }
}