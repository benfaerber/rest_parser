use anyhow::{anyhow, Context};
use indexmap::IndexMap;
use nom::{
    bytes::{complete::tag, streaming::take_until}, sequence::pair, IResult
};
use base64::{prelude::BASE64_STANDARD, Engine};
use std::str;

use crate::template::Template;

const AUTHORIZATION_HEADER: &str = "Authorization";

const CONTENT_TYPE: &str = "Content-Type";

pub(crate) struct RestHeaders {
    pub(crate) authorization: Option<Authorization>,
    pub(crate) headers: IndexMap<String, Template>
}

impl RestHeaders {
    /// `httparse` doesn't take ownership of the headers
    /// This is just coercing them into templates
    /// If an authentication header can be found and parsed,
    /// turn it into an Authorization struct
    pub(crate) fn from_header_slice(
        headers_slice: &mut [httparse::Header],
    ) -> anyhow::Result<Self> {
        let headers_vec: Vec<httparse::Header> = headers_slice
            .iter()
            .take_while(|h| !h.name.is_empty() && !h.value.is_empty())
            .map(|h| h.to_owned())
            .collect();

        let mut headers: IndexMap<String, Template> = IndexMap::new();
        let mut authorization: Option<Authorization> = None;
        for header in headers_vec {
            let name = header.name.to_string();
            let str_val = str::from_utf8(header.value)
                .context(format!("Cannot parse header {} as UTF8", name))?;

            // If successfully parse authentication from header, save it
            // If it can't be parsed, it will be included as a normal header
            if name.to_lowercase() == AUTHORIZATION_HEADER.to_lowercase() {
                if let Ok(auth) = Authorization::from_header(str_val) {
                    authorization = Some(auth);
                    continue;
                }
            }

            let value = Template::new(str_val);
            headers.insert(name, value);
        }

        Ok(Self {
            headers,
            authorization,
        })
    }

    pub(crate) fn content_type(&self) -> String {
        self.headers.get(CONTENT_TYPE)
            .unwrap_or(&Template::new("unknown"))
            .raw
            .clone()
    }
}


/// The `Authorization` header
#[derive(Debug, Clone, PartialEq)]
pub enum Authorization {
    Bearer(String),
    Basic {
        username: String,
        password: Option<String>,
    }
}

impl Authorization {
    /// Convert the value of an Authorization header into an authentication
    /// struct Can either be Bearer or Basic
    pub fn from_header(input: &str) -> anyhow::Result<Self> {
        fn bearer(input: &str) -> IResult<&str, &str> {
            tag("Bearer ")(input)
        }

        fn basic(input: &str) -> IResult<&str, &str> {
            tag("Basic ")(input)
        }

        fn username_and_password(input: &str) -> IResult<&str, &str> {
            let (password, (username, _)) =
                pair(take_until(":"), tag(":"))(input)?;
            Ok((username, password))
        }

        if let Ok((token, _)) = bearer(input) {
            return Ok(Self::Bearer(token.into()));
        }

        if let Ok((encoded, _)) = basic(input) {
            let decoded_bytes = BASE64_STANDARD.decode(encoded)?;
            let decoded = str::from_utf8(decoded_bytes.as_slice())?;

            let (username, password) = match username_and_password(decoded) {
                // There is a username and password seperated by a colon
                Ok((u, p)) => (u.into(), Some(p.into())),
                // There is just a username
                Err(_) => (decoded.into(), None),
            };

            return Ok(Self::Basic { username, password });
        }

        Err(anyhow!("Failed to parse auth header"))
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_auth_header_test() {
        let example = "Basic Zm9vOmJhcg==";
        match Authorization::from_header(example).unwrap() {
            Authorization::Basic { username, password } => {
                assert_eq!(username.to_string(), "foo");
                assert_eq!(password.unwrap().to_string(), "bar");
            }
            _ => panic!("Should be basic auth!"),
        };

        let example = "Basic dXNlcm5hbWV3aXRob3V0cGFzc3dvcmQ=";
        match Authorization::from_header(example).unwrap() {
            Authorization::Basic { username, password } => {
                assert_eq!(username.to_string(), "usernamewithoutpassword");
                assert!(password.is_none());
            }
            _ => panic!("Should be basic auth!"),
        };

        let example = "Bearer eyjlavljhhkjasdjlkhskljdfklasdlkjhf";
        match Authorization::from_header(example).unwrap() {
            Authorization::Bearer(bearer) => {
                assert_eq!(
                    bearer.to_string(),
                    "eyjlavljhhkjasdjlkhskljdfklasdlkjhf"
                )
            }
            _ => panic!("Should be bearer auth!"),
        }
    }
}
