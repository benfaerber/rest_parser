///! Parses a `.rest` or `.http` file
///! These files are used in many IDEs such as Jetbrains, VSCode, and
///! Visual Studio Jetbrains and nvim-rest call it `.http`
///! VSCode and Visual Studio call it `.rest`

use anyhow::{anyhow, Context};
use indexmap::IndexMap;
use nom::{
    bytes::{complete::tag, streaming::take_until}, character::complete::alphanumeric1, combinator::opt, error::Error as NomError, sequence::pair, IResult
};
use core::fmt;
use std::{path::Path, str::{self, FromStr}};
use url::Url;

use crate::template::Template;

use super::headers::{Authorization, RestHeaders};

type StrResult<'a> = Result<(&'a str, &'a str), nom::Err<NomError<&'a str>>>;

pub(crate) const REQUEST_NEWLINE: &str = "\r\n";
pub(crate) const BODY_DELIMITER: &str = "\r\n\r\n";

const FORM_URL_ENCODED: &str = "application/x-www-form-urlencoded";

pub type RestVariables = IndexMap<String, Template>;

/// The specific type of REST file.
/// They are all similar with slightly different feature sets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RestFlavor {
    Vscode,
    Jetbrains,
    #[default] 
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

impl fmt::Display for RestFlavor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let output = match self {
            Self::Vscode => "vscode",
            Self::Jetbrains => "jetbrains",
            Self::Generic => "generic",
        }; 
        write!(f, "{output}")
    }
}

const LOAD_SYMBOL: &str = "<"; 
const SAVE_SYMBOL: &str = ">>"; 
const VAR_SYMBOL: &str = "@"; 

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Text(Template),
    LoadFromFile {
        process_variables: bool,
        encoding: Option<String>,
        filepath: Template, 
    },
    SaveToFile {
        text: Template,
        filepath: Template,
    },
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
                filepath: Template::new(inp),
            }; 

            Ok(("", body))
        }

        fn parse_save_file(inp: &str) -> IResult<&str, Body> {
            let (inp, main_body) = take_until(SAVE_SYMBOL)(inp)?;
            let (inp, _) = tag(SAVE_SYMBOL)(inp)?;
            let (filepath, _) = tag(" ")(inp)?;
            
            let body = Body::SaveToFile { 
                text: Template::new(main_body.trim_end()),
                filepath: Template::new(filepath),
            };
            Ok(("", body)) 
        } 

        if let Ok((_, body)) = parse_from_file(input) {
            return body
        }

        if let Ok((_, body)) = parse_save_file(input) {
            return body
        }

        Body::Text(Template::new(input))
    }
}

#[derive(Debug, Clone, Default)]
pub struct RestRequest {
    pub name: Option<String>,
    pub url: Template,
    pub query: IndexMap<String, Template>,
    pub body: Option<Body>,
    pub method: Template,
    pub headers: IndexMap<String, Template>,
    pub authorization: Option<Authorization>,
    pub commands: IndexMap<String, Option<String>>,
}

impl RestRequest {
    /// Convert a name and a raw request into structured data 
    pub(crate) fn from_raw_request(
        name: Option<String>,
        commands: IndexMap<String, Option<String>>,
        raw_request: &str,
    ) -> anyhow::Result<Self> {
        let (req_portion, raw_body_portion) =
            parse_request_and_raw_body(raw_request.trim());

        // We need an empty buffer of headers (max of 64)
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);
       
        // Clean up vars from request so it can be parsed 
        let req_portion = Self::apply_placeholder(&req_portion, true);

        let req_buffer = req_portion.as_bytes();
        req.parse(req_buffer).map_err(|parse_err| {
            println!("{:?}", parse_err); 
            anyhow!("Failed to parse request! {parse_err:?}")
        })?;

        let path = req
            .path
            .ok_or(anyhow!("There is no path for this request!"))?;

        let path = Self::apply_placeholder(path, false);

        let RestUrl { url, query } = RestUrl::from_str(&path)?;
        let rest_headers = RestHeaders::from_header_slice(req.headers)?;
        let content_type = rest_headers.content_type(); 
        let RestHeaders { headers, authorization } = rest_headers;

        let method = Template::new(req.method.unwrap_or("GET"));
        
        let body = raw_body_portion.map(|body| Body::parse(&body, &content_type));

        Ok(Self {
            name,
            method,
            url,
            body,
            query,
            headers,
            authorization,
            commands,
        })
    }

    fn apply_placeholder(path: &str, apply: bool) -> String {
        let open_d = "{{ ";
        let close_d = " }}";
        let open_p = "_TO_";
        let close_p = "_TC_";

        let (rep1, rep2) = if apply {
            ((open_d, open_p),  (close_d, close_p))
        } else {
            ((open_p, open_d), (close_p, close_d))
        }; 

        path.replace(rep1.0, rep1.1)
            .replace(rep2.0, rep2.1)
    }
}

#[derive(Debug, Clone)]
struct RestUrl {
    url: Template,
    query: IndexMap<String, Template>,
}

/// Parse the query portion of a URL
///
/// This injects the query portion into a fake url
/// The template literals in the url would screw up parsing
/// I'd rather use a well tested crate than implementing query parsing
/// There's no public interface in URL to parse the query portion alone
fn parse_query(
    query_portion: &str,
) -> anyhow::Result<IndexMap<String, Template>> {
    let fake_url = Url::parse(&format!("http://localhost?{query_portion}"))
        .context(format!("Invalid query (Query: {query_portion})"))?;

    let mut query: IndexMap<String, Template> = IndexMap::new();
    for (k, v) in fake_url.query_pairs() {
        let template = Template::new(&v);
        query.insert(k.into(), template);
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
            let url = Template::new(url_part);
            let query = parse_query(query_part)?;

            Ok(Self { url, query })
        } else {
            let url: String = path.to_string().try_into()?;

            // The url is just a string or template
            Ok(Self {
                url: Template::new(&url), 
                query: IndexMap::new(),
            })
        }
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
            // TODO: Figure out how to deal with spaces in templates here (maybe regex transform? "{{ +" -> "XXX") 
            let req_portion = req_portion.replace("{{ ", "{{");
            let req_with_end = format!("{req_portion}{REQUEST_NEWLINE}");
            (req_with_end, Some(body_portion.trim().into()))
        }
        _ => (input.into(), None),
    }
}


#[cfg(test)]
mod test {
    use crate::template::TemplatePart;

    use super::*;
    use indoc::indoc;

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

        // With space
        let example = "{{ VAR}}?x={{ b }}&word=cool";
        let parsed = RestUrl::from_str(example).unwrap();
        assert_eq!(parsed.url.to_string(), "{{ VAR}}");
        assert_eq!(parsed.query.get("x").unwrap().to_string(), "{{ b }}");
        assert_eq!(parsed.query.get("word").unwrap().to_string(), "cool");
    }

    #[test]
    fn parse_request_and_raw_body_test() {
        let example = indoc! {r#"
            POST /post?q=hello HTTP/1.1
            Host: localhost
            Content-Type: application/json
            X-Http-Method-Override: PUT

            {
                "data": "my data"
            }
        "#}.trim().replace("\n", "\r\n");

        let (req, body) = parse_request_and_raw_body(&example);

        let expected = indoc! {r#"
            POST /post?q=hello HTTP/1.1
            Host: localhost
            Content-Type: application/json
            X-Http-Method-Override: PUT
        "#}; 

        assert_eq!(
            req,
            expected.replace("\n", "\r\n")
        );

        assert_eq!(
            body,
            Some(
                indoc! {r#"{
                    "data": "my data"
                }"#}
                .replace("\n", "\r\n")
            )
        );
    }
    
    #[test]
    fn parse_body_test() {
        let content_type = "text/plain"; 
        let normal_body = "blah blah blah\nasdfasdf";
        fn text(t: &str) -> Body {
            Body::Text(Template::new(t))
        }

        assert_eq!(Body::parse(normal_body, content_type), text(normal_body)); 
       
        let file_import = "< file.txt";
        assert_eq!(Body::parse(file_import, content_type), Body::LoadFromFile {
            process_variables: false,
            encoding: None,
            filepath: Template::new("file.txt")
        });

        let file_import_with_vars = "<@ file.txt";
        assert_eq!(Body::parse(file_import_with_vars, content_type), Body::LoadFromFile {
            process_variables: true,
            encoding: None,
            filepath: Template::new("file.txt")
        });

        let file_import_with_vars_encoding = "<@latin1 file.txt";
        assert_eq!(Body::parse(file_import_with_vars_encoding, content_type), Body::LoadFromFile {
            process_variables: true,
            encoding: Some("latin1".to_string()),
            filepath: Template::new("file.txt")
        });
       
        let json_with_export = indoc! {r#"
            {
                "data": "my data"
            }

            >> ./cool-file.json"#};
        assert_eq!(Body::parse(json_with_export, "application/json"), Body::SaveToFile { 
            text: Template::new(indoc! {r#"
                {
                    "data": "my data"
                }"#}),
            filepath: Template::new("./cool-file.json")
        });


        let form_body = indoc! {r#"
            a=1&
            b=2&
            c=3
        "#};
        assert_eq!(Body::parse(form_body, FORM_URL_ENCODED), text("a=1&b=2&c=3"));
    }

    #[test]
    fn parse_get_request_test() {
        let get_request = indoc! {r#"
            GET https://httpbin.org/get HTTP/1.1
        "#};

        let req = RestRequest::from_raw_request(None, IndexMap::new(), get_request);
        match req {
            Ok(RestRequest { url, method, .. }) => {
                assert_eq!(url.to_string(), "https://httpbin.org/get");
                assert_eq!(method.to_string(), "GET");
            },
            other => panic!("Failure!, {other:?}")
        }

        // Test Var
        let get_request = indoc! {r#"
            GET {{HOST}}/get HTTP/1.1
        "#};

        let req = RestRequest::from_raw_request(None, IndexMap::new(), get_request);
        match req {
            Ok(RestRequest { url, method, .. }) => {
                assert_eq!(url.parts.first(), Some(&TemplatePart::var("HOST")));
                assert_eq!(method.to_string(), "GET");
            },
            other => panic!("Failure!, {other:?}")
        }
        
        // Test Var with Space
        let get_request = indoc! {r#"
            GET {{ HOST }}/get HTTP/1.1
        "#};

        let req = RestRequest::from_raw_request(None, IndexMap::new(), get_request);
        match req {
            Ok(RestRequest { url, method, .. }) => {
                assert_eq!(url.parts.first(), Some(&TemplatePart::var("HOST")));
                assert_eq!(method.to_string(), "GET");
            },
            other => panic!("Failure!, {other:?}")
        }
    }
}
