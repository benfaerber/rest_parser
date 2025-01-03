use std::str::FromStr;
use anyhow::{Error, anyhow};
use nom::{
    bytes::{complete::tag, streaming::take_until}, character::complete::space0, IResult
};
use crate::RestVariables;

use super::lexer::parse_variable_identifier;
use std::fmt;

pub type TemplateMap = indexmap::IndexMap<String, Template>;

#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    Text(String),
    Variable(String),
}

impl TemplatePart {
    pub fn text(value: &str) -> Self {
        TemplatePart::Text(value.to_string())
    }

    pub fn var(value: &str) -> Self {
        TemplatePart::Variable(value.to_string())
    }    
}

const VARIABLE_START: &str = "{{";
const VARIABLE_END: &str = "}}";

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Template {
    pub parts: Vec<TemplatePart>,
    pub raw: String,
}

impl Template {
    pub fn new(value: &str) -> Self {
        Self::from_str(value)
            .unwrap_or(Self {
                parts: vec![
                    TemplatePart::text(value)
                ],
                raw: value.into(),
            })
    } 

    /// Takes a variable context and renders a template
    /// Useful if your application doesn't require variables and you want them rendered now
    pub fn render(&self, variables: &RestVariables) -> String {
        let mut built = "".to_string(); 
        for part in &self.parts {
            built += match part {
                TemplatePart::Variable(name) => match variables.get(name) {
                    Some(value) => value.raw.as_str(),
                    None => "",
                },
                TemplatePart::Text(text) => text.as_str(),
            };
        }
        built
    }
}

impl FromStr for Template {
    type Err = Error; 

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        fn parse_variable(inp: &str) -> IResult<&str, &str> {
            let (inp, _) = tag(VARIABLE_START)(inp)?;
            let (inp, _) = space0(inp)?;
            let (inp, var) = parse_variable_identifier(inp)?;
            let (inp, _) = space0(inp)?;
            let (inp, _) = tag(VARIABLE_END)(inp)?;
            Ok((inp, var))
        }

        fn parse_text(inp: &str) -> IResult<&str, &str> {
            take_until(VARIABLE_START)(inp)
        }

        let mut parts: Vec<TemplatePart> = vec![];
        let mut value = s.to_string(); 

        while !value.is_empty() {
            let test_val = &value.clone();
            if let Ok((new_val, var)) = parse_variable(test_val) {
                value = new_val.to_string();
                parts.push(TemplatePart::var(var));
                continue;
            } 

            if let Ok((new_val, text)) = parse_text(test_val) {
                if text.is_empty() {
                    return Err(anyhow!("Unclosed template!"));
                } 

                value = new_val.to_string();
                parts.push(TemplatePart::text(text));
                continue;
            }
           
            parts.push(TemplatePart::text(&value));
            break; 
        }

        let raw = s.into();
        Ok(Template {
            parts,
            raw,
        })
    }
}

impl From<String> for Template {
    fn from(value: String) -> Self {
        Template::new(&value)
    }
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_template() {
        use indexmap::IndexMap; 
        fn var(t: &str) -> TemplatePart {
            TemplatePart::Variable(t.into())
        } 

        fn text(t: &str) -> TemplatePart {
            TemplatePart::Text(t.into())
        }

        let line = "hello {{name}}! swag";
        let template = Template::new(line);
        assert_eq!(template.parts, vec![
            text("hello "), 
            var("name"),
            text("! swag"), 
        ]);

        let vars: IndexMap<String, Template> = {
            let mut m = IndexMap::new();
            m.insert("name".into(), Template::new("Joe"));
            m
        }; 
        
        let render = template.render(&vars); 
        assert_eq!(render, "hello Joe! swag".to_string());

        let line = "{{ name}}";
        let got = Template::from_str(line).unwrap();
        assert_eq!(got.parts, vec![
            var("name"),
        ]);

        let line = "{{first }} {{ last }}";
        let got = Template::from_str(line).unwrap();
        assert_eq!(got.parts, vec![
            var("first"),
            text(" "), 
            var("last"),
        ]);
    }

    #[test]
    fn can_parse_error() {
        // This should unclosed template error
        let template = Template::from_str("Test {{ end"); 
        assert!(template.is_err());

        // This should error
        let template = Template::from_str("Test {{{}} end"); 
        assert!(template.is_err());

        // Just parse as normal text
        let template = Template::from_str("Test }} end"); 
        assert!(template.is_ok());
    }
}
