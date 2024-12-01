///! Parses a `.rest` or `.http` file
///! These files are used in many IDEs such as Jetbrains, VSCode, and
///! Visual Studio Jetbrains and nvim-rest call it `.http`
///! VSCode and Visual Studio call it `.rest`

use anyhow::{anyhow, Context};
use indexmap::IndexMap;
use nom::{
    bytes::{complete::tag, streaming::take_until}, character::complete::alphanumeric1, combinator::opt, error::Error as NomError, sequence::pair, IResult
};
use base64::{prelude::BASE64_STANDARD, Engine};
use std::{fs::File, io::Read, path::Path, str::{self, FromStr}};
use url::Url;

use super::lexer::{Line, parse_lines};

type StrResult<'a> = Result<(&'a str, &'a str), nom::Err<NomError<&'a str>>>;

const REQUEST_NEWLINE: &str = "\r\n";
const BODY_DELIMITER: &str = "\r\n\r\n";

const AUTHORIZATION_HEADER: &str = "Authorization";

pub type RestVariables = IndexMap<String, String>;

/// The specific type of REST file.
/// They are all similar with slightly different feature sets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestFlavor {
    Vscode,
    Jetbrains,
    Generic,
}

impl RestFlavor {
    fn from_path(path: impl AsRef<Path>) -> Self {
        match path.as_ref().extension() {
            Some(ext) if ext == "http" => Self::Jetbrains,
            Some(ext) if ext == "rest" => Self::Vscode,
            _ => Self::Generic,
        }
    }
}

const LOAD_SYMBOL: &str = "<"; 
const SAVE_SYMBOL: &str = ">>"; 
const VAR_SYMBOL: &str = "@"; 

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    Text(String),
    FromFile {
        process_variables: bool,
        encoding: Option<String>,
        filepath: String, 
    },
    SaveFile {
        text: String,
        filepath: String,
    }
}


impl Body {
    fn parse(input: &str) -> Self {
        fn parse_from_file(inp: &str) -> IResult<&str, Body> {
            let (inp, _) = tag(LOAD_SYMBOL)(inp)?;
            
            let (inp, at_sign) = opt(tag(VAR_SYMBOL))(inp)?;
            let process_variables = at_sign.is_some();

            let (inp, encoding) = opt(alphanumeric1)(inp)?;
            let encoding = encoding.map(|e| e.to_string());

            // A space seperates the optional encoding and the filepath 
            let (inp, _) = tag(" ")(inp)?;

            let body = Body::FromFile { 
                process_variables,
                encoding,
                filepath: inp.to_string()
            }; 

            Ok(("", body))
        }

        fn parse_save_file(inp: &str) -> IResult<&str, Body> {
            let (inp, main_body) = take_until(SAVE_SYMBOL)(inp)?;
            let (inp, _) = tag(SAVE_SYMBOL)(inp)?;
            let (filepath, _) = tag(" ")(inp)?;
            
            let body = Body::SaveFile { 
                text: main_body.to_string(), 
                filepath: filepath.to_string(), 
            };
            Ok(("", body)) 
        } 

        if let Ok((_, body)) = parse_from_file(input) {
            return body
        }

        if let Ok((_, body)) = parse_save_file(input) {
            return body
        }

        Body::Text(input.into())
    }
}

#[derive(Debug, Clone)]
pub struct RestRequest {
    pub name: Option<String>,
    pub url: String,
    pub query: IndexMap<String, String>,
    pub body: Option<Body>,
    pub method: String,
    pub headers: IndexMap<String, String>,
    pub authorization: Option<Authorization>,
}

impl RestRequest {
    /// Convert a name and a raw request into structured data 
    fn from_raw_request(
        name: Option<String>,
        raw_request: &str,
    ) -> anyhow::Result<Self> {
        let (req_portion, raw_body_portion) =
            parse_request_and_raw_body(raw_request.trim());

        // We need an empty buffer of headers (max of 64)
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);
        
        let req_buffer = req_portion.as_bytes();
        req.parse(req_buffer).map_err(|parse_err| {
            anyhow!("Failed to parse request! {parse_err:?}")
        })?;

        let path = req
            .path
            .ok_or(anyhow!("There is no path for this request!"))?;

        let RestUrl { url, query } = RestUrl::from_str(path)?;
        let RestHeaders { headers, authorization } = RestHeaders::from_header_slice(req.headers)?;

        let method = req.method.unwrap_or("GET").into();
        let body = raw_body_portion.map(|body| Body::parse(&body));

        Ok(Self {
            name,
            method,
            url,
            body,
            query,
            headers,
            authorization,
        })
    }
}

struct RestHeaders {
    authorization: Option<Authorization>,
    headers: IndexMap<String, String>
}

impl RestHeaders {
    /// `httparse` doesn't take ownership of the headers
    /// This is just coercing them into templates
    /// If an authentication header can be found and parsed,
    /// turn it into an Authorization struct
    fn from_header_slice(
        headers_slice: &mut [httparse::Header],
    ) -> anyhow::Result<Self> {
        let headers_vec: Vec<httparse::Header> = headers_slice
            .iter()
            .take_while(|h| !h.name.is_empty() && !h.value.is_empty())
            .map(|h| h.to_owned())
            .collect();

        let mut headers: IndexMap<String, String> = IndexMap::new();
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

            let value = str_val.to_string();
            headers.insert(name, value);
        }

        Ok(Self {
            headers,
            authorization,
        })
    }
}


#[derive(Debug, Clone)]
struct RestUrl {
    url: String,
    query: IndexMap<String, String>,
}

/// Parse the query portion of a URL
///
/// This injects the query portion into a fake url
/// The template literals in the url would screw up parsing
/// I'd rather use a well tested crate than implementing query parsing
/// There's no public interface in URL to parse the query portion alone
fn parse_query(
    query_portion: &str,
) -> anyhow::Result<IndexMap<String, String>> {
    let fake_url = Url::parse(&format!("http://localhost?{query_portion}"))
        .context(format!("Invalid query (Query: {query_portion})"))?;

    let mut query: IndexMap<String, String> = IndexMap::new();
    for (k, v) in fake_url.query_pairs() {
        query.insert(k.into(), v.into());
    }
    Ok(query)
}

impl FromStr for RestUrl {
    type Err = anyhow::Error;

    fn from_str(path: &str) -> Result<Self, Self::Err> {
        fn url_and_query(input: &str) -> StrResult {
            let (query, (url, _)) = pair(take_until("?"), tag("?"))(input)?;
            Ok((url, query))
        }

        if let Ok((url_part, query_part)) = url_and_query(path) {
            let url = url_part.to_string();
            let query = parse_query(query_part)?;

            return Ok(Self { url, query });
        }

        // The url is just a string or template
        Ok(Self {
            url: path.to_string().try_into()?,
            query: IndexMap::new(),
        })
    }
}

/// A basic representaion of the REST format
#[derive(Debug, Clone)]
pub struct RestFormat {
    /// A list of recipes
    pub requests: Vec<RestRequest>,
    /// Variables used for templating
    pub variables: IndexMap<String, String>,
    /// The specific flavor of REST format (VSCode, Jetbrains, etc.)
    pub flavor: RestFlavor,
}

/// `httparse` does not parse bodies
/// We need to seperate them from the request portion
fn parse_request_and_raw_body(input: &str) -> (String, Option<String>) {
    fn take_until_body(raw: &str) -> IResult<&str, String> {
        let (raw, (init_body, rest)) = pair(
            take_until(BODY_DELIMITER),
            opt(pair(tag(SAVE_SYMBOL), take_until(BODY_DELIMITER)))
        )(raw)?;

        let addition = match rest {
            Some((a, b)) => format!("{a}{b}"),
            None => "".to_string()
        };

        let full_body = format!("{init_body}{addition}");

        Ok((raw, full_body))  
    }

    match take_until_body(input) {
        Ok((body_portion, req_portion)) => {
            let req_with_end = format!("{req_portion}{REQUEST_NEWLINE}");
            (req_with_end, Some(body_portion.trim().into()))
        }
        _ => (input.into(), None),
    }
}

impl RestFormat {
    pub fn parse_file(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let flavor = RestFlavor::from_path(&path); 
        let path = path.as_ref();

        let mut file = File::open(path)
            .context(format!("Error opening REST file {path:?}"))?;

        let mut text = String::new();
        file.read_to_string(&mut text)
            .context(format!("Error reading REST file {path:?}"))?;

        Self::parse(&text, flavor)
    }

    pub fn parse(text: &str, flavor: RestFlavor) -> anyhow::Result<Self> {
        let (lines, variables) = parse_lines(text)?;
        Ok(Self::from_lines(lines, variables, flavor)?)
    }

    /// Take each parsed line (like a lex token) and
    /// convert it to the REST format
    fn from_lines(
        lines: Vec<Line>,
        variables: IndexMap<String, String>,
        flavor: RestFlavor,
    ) -> anyhow::Result<Self> {
        let mut requests: Vec<RestRequest> = vec![];
        let mut current_name: Option<String> = None;
        let mut current_request: String = "".into();
        for line in lines {
            match line {
                Line::Seperator(name_opt) => {
                    if current_request.trim() != "" {
                        let request= RestRequest::from_raw_request(
                            current_name,
                            &current_request,
                        )?;
                        requests.push(request);
                    }

                    current_name = None;
                    current_request = "".into();

                    if let Some(name) = name_opt {
                        current_name = Some(name);
                    }
                }
                Line::Name(name) => {
                    current_name = Some(name);
                }
                Line::Request(req) => {
                    let next_line = format!("{req}{REQUEST_NEWLINE}");
                    current_request.push_str(&next_line);
                }
            }
        }

        let request = RestRequest::from_raw_request(
            current_name,
            &current_request,
        )?;
        requests.push(request);

        Ok(Self { requests, variables, flavor })
    }
}

impl FromStr for RestFormat {
    type Err = anyhow::Error;
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let (lines, variables) = parse_lines(text)?;
        // TODO: Figure out flavor
        Ok(Self::from_lines(lines, variables, RestFlavor::Vscode)?)
    }
}

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
    fn parse_url_test() {
        let example = "{{VAR}}?x={{b}}&word=cool";
        let parsed = RestUrl::from_str(example).unwrap();
        assert_eq!(parsed.url.to_string(), "{{VAR}}");
        assert_eq!(parsed.query.get("x").unwrap().to_string(), "{{b}}");
        assert_eq!(parsed.query.get("word").unwrap().to_string(), "cool");

        let example = "https://example.com";
        let parsed: RestUrl = example.parse().unwrap();
        assert_eq!(parsed.url.to_string(), "https://example.com");
        assert_eq!(parsed.query.len(), 0);

        let example = "https://example.com?q={{query}}";
        let parsed: RestUrl = example.parse().unwrap();
        assert_eq!(parsed.url.to_string(), "https://example.com");
        assert_eq!(parsed.query.get("q").unwrap().to_string(), "{{query}}");

        let example = "{{my_url}}";
        let parsed: RestUrl = example.parse().unwrap();
        assert_eq!(parsed.url.to_string(), "{{my_url}}");
    }

    #[test]
    fn parse_request_and_raw_body_test() {
        let example = r#"
POST /post?q=hello HTTP/1.1
Host: localhost
Content-Type: application/json
X-Http-Method-Override: PUT

{
    "data": "my data"
}
"#
        .trim()
        .replace("\n", REQUEST_NEWLINE);

        let (req, body) = parse_request_and_raw_body(&example);

        assert_eq!(
            req,
            r#"POST /post?q=hello HTTP/1.1
Host: localhost
Content-Type: application/json
X-Http-Method-Override: PUT
"#
            .replace("\n", "\r\n")
        );

        assert_eq!(
            body,
            Some(
                r#"{
    "data": "my data"
}"#
                .replace("\n", "\r\n")
            )
        );
    }

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

    #[test]
    fn parse_body_test() {
        let normal_body = "blah blah blah\nasdfasdf";
        assert_eq!(Body::parse(normal_body), Body::Text(normal_body.to_string()));
       
        let file_import = "< file.txt";
        assert_eq!(Body::parse(file_import), Body::FromFile {
            process_variables: false,
            encoding: None,
            filepath: "file.txt".to_string(),
        });

        let file_import_with_vars = "<@ file.txt";
        assert_eq!(Body::parse(file_import_with_vars), Body::FromFile {
            process_variables: true,
            encoding: None,
            filepath: "file.txt".to_string(),
        });

        let file_import_with_vars_encoding = "<@latin1 file.txt";
        assert_eq!(Body::parse(file_import_with_vars_encoding), Body::FromFile {
            process_variables: true,
            encoding: Some("latin1".to_string()),
            filepath: "file.txt".to_string(),
        });

    }
}
