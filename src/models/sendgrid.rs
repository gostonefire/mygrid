use std::str::FromStr;
use serde::{Deserialize, Serialize};
use crate::manager_mail::MailError;

#[derive(Serialize, Deserialize)]
pub struct Content {
    #[serde(rename = "type")]
    pub content_type: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Address {
    pub email: String,
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct Personalizations {
    pub to: Vec<Address>,
}

#[derive(Serialize, Deserialize)]
pub struct Email {
    pub personalizations: Vec<Personalizations>,
    pub from: Address,
    pub subject: String,
    pub content: Vec<Content>,
}

impl FromStr for Address {
    type Err = MailError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fn init_cap(s: &str) -> String {
            let mut c = s.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        }

        let p = s.split('<').collect::<Vec<&str>>();
        if p.len() == 1 {
            let p2 = p[0].split('@').collect::<Vec<&str>>();
            if p2.len() == 2 {
                let name = p2[0]
                    .split('.')
                    .collect::<Vec<&str>>()
                    .iter()
                    .map(|&n| init_cap(n))
                    .fold(String::new(), |acc, s| acc + " " + &s);

                return Ok(Address { email: s.to_string(), name })
            }
        } else if p.len() == 2 {
            return Ok(Address{
                email: p[1].trim().replace(">", "").to_string(),
                name: p[0].trim().to_string() })
        }

        Err(MailError::InvalidEmailAddress(s.to_string()))
    }
}
