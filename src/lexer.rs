use indexmap::IndexMap;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_till},
    character::complete::{
        alpha1, alphanumeric1, char, newline, space0, space1,
    },
    combinator::{opt, recognize},
    multi::many0_count,
    sequence::{pair, tuple},
    IResult, Parser,
};
use std::str;

use crate::{template::Template, RestVariables};

type StrResult<'a> = IResult<&'a str, &'a str>;


const REQUEST_DELIMITER: &str = "###";

const NAME_ANNOTATION: &str = "@name";
const COMMAND_ANNOTATION: &str = "@";

/// A single line during parsing
/// This is the equivalent of a lex token
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    /// A section seperator:
    /// `### RequestName` or `###`
    Seperator(Option<String>),
    
    /// A request name annotation:
    /// `# @name RequestName`
    Name(String),
    
    /// A special command for a request
    /// `# @no-log` or `# @timeout 300`
    Command {
        name: String,
        params: Option<String>,
    },

    /// A single line of a request:
    /// `POST https://example.com HTTP/1.1`
    Request(String),
}

/// Attempt to parse an optionally named seperator
/// `### {optional_name}`
fn parse_seperator(input: &str) -> IResult<&str, Option<String>> {
    let (input, _) = tag(REQUEST_DELIMITER)(input)?;
    let (input, req_name) =
        opt(pair(space1, take_till(|c| c == ' ' || c == '\n')))(input)?;

    let potential_name = req_name.map(|(_, name)| name.to_string());
    Ok((input, potential_name))
}

/// A comment can start with `//` or `#`
fn starting_comment(line: &str) -> StrResult {
    alt((tag("//"), tag("#")))(line)
}

/// Attempt to parse a name annotation
/// `# @name RequestName`
fn parse_request_name_annotation(input: &str) -> IResult<&str, &str> {
    let (input, _) = pair(starting_comment, space0)(input)?;
    let (input, _) = tag(NAME_ANNOTATION)(input)?;
    let (input, _) = pair(alt((char('='), char(' '))), space0)(input)?;
    let (input, req_name) = take_till(|c| c == ' ' || c == '\n')(input)?;

    Ok((input, req_name.into()))
}


/// Attempt to parse a name annotation
/// `# @no-log`
/// `# @connection-timeout 2 m`
fn parse_request_command(input: &str) -> IResult<&str, (&str, Option<&str>)> {
    let (input, _) = pair(starting_comment, space0)(input)?;
    let (input, _) = tag(COMMAND_ANNOTATION)(input)?;
    let (input, cmd_name) = take_till(|c| c == ' ' || c == '\n')(input)?;
    let (input, _) = space0(input)?; 
    let (input, params) = opt(take_till(|c| c == '\n'))(input)?;

    let params = match params {
        Some("") => None,
        other => other,
    };
    
    Ok((input, (cmd_name.into(), params)))
}


pub fn parse_variable_identifier(input: &str) -> IResult<&str, &str> {
    recognize(pair(
        alpha1,
        many0_count(alt((alphanumeric1, tag("_"), tag("-"), tag(".")))),
    ))
    .parse(input)
}


/// Parses an HTTP File variable
/// `@my_variable = hello`
fn parse_variable_assignment(input: &str) -> IResult<&str, (&str, &str)> {
    let (input, _) = char('@')(input)?;
    let (input, id) = parse_variable_identifier(input)?;

    let (input, _) = tuple((opt(space1), char('='), opt(space1)))(input)?;
    let (input, value) = take_till(|c| c == '\n')(input)?;
    let (input, _) = newline(input)?;

    Ok((input, (id.into(), value.into())))
}

/// A comment can start with `//` or `#`
/// A comment cannot be mid line because it messes with URLs
fn is_comment(line: &str) -> bool {
    matches!(starting_comment(line), Ok(_))
}

/// Parse an input string line by line
pub fn parse_lines(
    input: &str,
) -> anyhow::Result<(Vec<Line>, RestVariables)> {
    let mut lines: Vec<Line> = vec![];
    let mut variables: IndexMap<String, Template> = IndexMap::new();
    for line in input.trim().lines() {
        let line = &format!("{line}\n");
        if let Ok((_, seperator_name)) = parse_seperator(line) {
            lines.push(Line::Seperator(seperator_name));
            continue;
        }

        if let Ok((_, name)) = parse_request_name_annotation(line) {
            lines.push(Line::Name(name.into()));
            continue;
        }

        if let Ok((_, (name, params))) = parse_request_command(line) {
            lines.push(Line::Command {
                name: name.to_string(),
                params: params.map(|x| x.to_string()),
            });
            continue;
        }

        // Now that all the things that look like comments have been parsed,
        // we can remove the comments
        if is_comment(line) {
            continue
        }

        if let Ok((_, (key, val))) = parse_variable_assignment(line) {
            variables.insert(key.into(), Template::new(val));
            continue;
        }

        lines.push(Line::Request(line.trim().into()));
    }
    Ok((lines, variables))
}


#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse_http_variable() {
        let example_var = "@MY_VAR    = 1231\n";
        let (_, var) = parse_variable_assignment(example_var).unwrap();

        assert_eq!(var, ("MY_VAR", "1231"));

        let example_var = "@MY_NAME =hello\n";
        let (rest, var) = parse_variable_assignment(example_var).unwrap();

        assert_eq!(var, ("MY_NAME", "hello"));
        assert_eq!(rest, "");

        let example_var = "@Cool-Word = super_cool\n";
        let (_, var) = parse_variable_assignment(example_var).unwrap();

        assert_eq!(var, ("Cool-Word", "super_cool"));
    }

    #[test]
    fn parse_seperator_line() {
        let line = "### RequestName";
        let (_, name_opt) = parse_seperator(line).unwrap();
        assert_eq!(name_opt, Some("RequestName".into()));

        let line = "#######";
        let (_, name_opt) = parse_seperator(line).unwrap();
        assert_eq!(name_opt, None);

        let line = "###";
        let (_, name_opt) = parse_seperator(line).unwrap();
        assert_eq!(name_opt, None);

        let line = "#";
        let res = parse_seperator(line);
        assert!(res.is_err());
    }

    #[test]
    fn parse_request_name_test() {
        let line = "# @name=hello";
        let (_, name) = parse_request_name_annotation(line).unwrap();
        assert_eq!(name, "hello".to_string());

        let line = "# @name Cool";
        let (_, name) = parse_request_name_annotation(line).unwrap();
        assert_eq!(name, "Cool".to_string());

        let line = "# a comment";
        assert!(parse_request_name_annotation(line).is_err());
    }

    #[test]
    fn parse_request_command_test() {
        let line = "# @no-log";
        let (_, out) = parse_request_command(line).unwrap();
        assert_eq!(out, ("no-log", None));

        let line = "# @timeout 100";
        let (_, out) = parse_request_command(line).unwrap();
        assert_eq!(out, ("timeout", Some("100")));

        let line = "# @connection-timeout 2 m";
        let (_, out) = parse_request_command(line).unwrap();
        assert_eq!(out, ("connection-timeout", Some("2 m")));
    }
}
