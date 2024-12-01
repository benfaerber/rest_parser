///! Parses a `.rest` or `.http` file
///! These files are used in many IDEs such as Jetbrains, VSCode, and
///! Visual Studio Jetbrains and nvim-rest call it `.http`
///! VSCode and Visual Studio call it `.rest`

use anyhow::{anyhow, Context};
use indexmap::IndexMap;
use nom::{
    bytes::{complete::tag, streaming::take_until}, character::complete::alphanumeric1, combinator::opt, error::Error as NomError, sequence::pair, IResult
};
use std::{path::Path, str::{self, FromStr}};
use url::Url;

use super::headers::{Authorization, RestHeaders};

type StrResult<'a> = Result<(&'a str, &'a str), nom::Err<NomError<&'a str>>>;

pub(crate) const REQUEST_NEWLINE: &str = "\r\n";
pub(crate) const BODY_DELIMITER: &str = "\r\n\r\n";

const FORM_URL_ENCODED: &str = "application/x-www-form-urlencoded";

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
    pub(crate) fn from_path(path: impl AsRef<Path>) -> Self {
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
    LoadFromFile {
        process_variables: bool,
        encoding: Option<String>,
        filepath: String, 
    },
    SaveToFile {
        text: String,
        filepath: String,
    }
}


impl Body {
    fn parse(input: &str, content_type: &str) -> Self {
        let input = if content_type == FORM_URL_ENCODED {
            &input.replace("\r\n", "").replace("\n", "")
        } else {
            input
        };

        fn parse_from_file(inp: &str) -> IResult<&str, Body> {
            let (inp, _) = tag(LOAD_SYMBOL)(inp)?;
            
            let (inp, at_sign) = opt(tag(VAR_SYMBOL))(inp)?;
            let process_variables = at_sign.is_some();

            let (inp, encoding) = opt(alphanumeric1)(inp)?;
            let encoding = encoding.map(|e| e.to_string());

            // A space seperates the optional encoding and the filepath 
            let (inp, _) = tag(" ")(inp)?;

            let body = Body::LoadFromFile { 
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
            
            let body = Body::SaveToFile { 
                text: main_body.trim_end().to_string(),
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

    /// Just get the text of the body
    /// Ignoring any saving and loading features. 
    pub fn text(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::LoadFromFile { .. } => "".into(),
            Self::SaveToFile { text, .. } => text.clone(),
        } 
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
    pub(crate) fn from_raw_request(
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
        let rest_headers = RestHeaders::from_header_slice(req.headers)?;
        let content_type = rest_headers.content_type(); 
        let RestHeaders { headers, authorization } = rest_headers;
        

        let method = req.method.unwrap_or("GET").into();
        
        let body = raw_body_portion.map(|body| Body::parse(&body, &content_type));

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
    fn parse_body_test() {
        let content_type = "text/plain"; 
        let normal_body = "blah blah blah\nasdfasdf";
        assert_eq!(Body::parse(normal_body, content_type), Body::Text(normal_body.to_string()));
       
        let file_import = "< file.txt";
        assert_eq!(Body::parse(file_import, content_type), Body::LoadFromFile {
            process_variables: false,
            encoding: None,
            filepath: "file.txt".to_string(),
        });

        let file_import_with_vars = "<@ file.txt";
        assert_eq!(Body::parse(file_import_with_vars, content_type), Body::LoadFromFile {
            process_variables: true,
            encoding: None,
            filepath: "file.txt".to_string(),
        });

        let file_import_with_vars_encoding = "<@latin1 file.txt";
        assert_eq!(Body::parse(file_import_with_vars_encoding, content_type), Body::LoadFromFile {
            process_variables: true,
            encoding: Some("latin1".to_string()),
            filepath: "file.txt".to_string(),
        });
        
        let json_with_export = r#"{
    "data": "my data"
}

>> ./cool-file.json"#;
        assert_eq!(Body::parse(json_with_export, "application/json"), Body::SaveToFile { 
            text: r#"{
    "data": "my data"
}"#.to_string(), 
            filepath: "./cool-file.json".to_string(), 
        });


        let form_body = r#"a=1&
b=2&
c=3
"#;
        assert_eq!(Body::parse(form_body, FORM_URL_ENCODED), Body::Text("a=1&b=2&c=3".into()));
    }
}
