use std::str::FromStr;
use std::io::Read;
use std::fs::File;
use std::path::Path;

use anyhow::Context;
use indexmap::IndexMap;

use super::lexer::{Line, parse_lines};
use super::parser::{RestRequest, RestFlavor, REQUEST_NEWLINE};

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
        let mut current_commands: IndexMap<String, Option<String>> = IndexMap::new();
        
        for line in lines {
            match line {
                Line::Seperator(name_opt) => {
                    if current_request.trim() != "" {
                        let request = RestRequest::from_raw_request(
                            current_name,
                            current_commands.clone(),
                            &current_request,
                        )?;
                        requests.push(request);
                    }

                    current_name = None;
                    current_request = "".into();
                    current_commands = IndexMap::new();

                    if let Some(name) = name_opt {
                        current_name = Some(name);
                    }
                }
                Line::Name(name) => {
                    current_name = Some(name);
                },
                Line::Command { name, params } => {
                    current_commands.insert(name, params); 
                },
                Line::Request(req) => {
                    current_request.push_str(&req);
                    current_request.push_str(REQUEST_NEWLINE);
                }
            }
        }

        let request = RestRequest::from_raw_request(
            current_name,
            current_commands,
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
