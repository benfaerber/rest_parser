use rest_parser::{Body, RestFormat, RestRequest, RestVariables};
use rest_parser::template::{Template, TemplateMap, TemplatePart};
use std::fs;

struct CurlRenderer {
    vars: RestVariables,
}
 
impl CurlRenderer {
    pub fn new(variables: Option<RestVariables>) -> Self {
        let vars = variables.unwrap_or(RestVariables::new());
        Self { vars }
    }

    fn render_query(&self, query: TemplateMap) -> String {
        let params = query.iter()
            .map(|(k, v)| format!("{}={}", k, self.render_template(v)))
            .collect::<Vec<String>>()
            .join("&");

        if params.is_empty() {
            "".to_string()
        } else {
            format!("?{params}")
        }
    }

    fn load_body_from_file(&self, filepath: Template, process_variables: bool) -> String {
        let filepath = filepath.render(&self.vars); 
        let raw = fs::read_to_string(&filepath).expect("Invalid file!");
        if process_variables {
            let template = Template::new(&raw);
            self.render_template(&template)
        } else {
            raw
        }
    }

    fn render_body(&self, opt_body: Option<Body>) -> (String, String) {
        let mut save_to = None; 
        let rendered_body = opt_body.map(|body| match body {
            Body::Text(text) => self.render_template(&text), 
            Body::LoadFromFile { filepath, process_variables, .. } => self.load_body_from_file(filepath, process_variables),
            Body::SaveToFile { text, filepath } => {
                save_to = Some(self.render_template(&filepath));
                self.render_template(&text)
            },
        });

        let out_body = rendered_body.map(|body_text| {
            let encoded_body = body_text
                .replace("&\r\n", "")
                .replace("\n", "")
                .replace("\"", "\\\"");
            format!(" -d \"{}\"", encoded_body)
        }).unwrap_or("".into());

        let save_cmd = save_to
            .map(|filename| format!(" -o \"{filename}\""))
            .unwrap_or("".into());

        (out_body, save_cmd)
    }

    fn render_headers(&self, headers: TemplateMap) -> String {
        let all_headers = headers
            .iter()
            .map(|(k, v)| format!("-H \"{}: {}\"", k, self.render_template(v)))
            .collect::<Vec<String>>()
            .join(" ");
        format!(" {all_headers}")
    }

    fn render_url(&self, url: Template) -> String {
        let rendered = self.render_template(&url); 
        format!("\"{rendered}\"")    
    }

    fn render_method(&self, method: Template) -> String {
        let method = self.render_template(&method); 
        format!(" -X {method}")
    }

    fn render_variables(&self) -> String {
        let all_vars = self.vars.iter().map(|(k, v)| {
            format!("{}=\"{}\"", k, self.render_template(v)) 
        }).collect::<Vec<String>>().join("; ");
        format!("{all_vars}; ")
    }

    fn render_template(&self, template: &Template) -> String {
        template.parts.iter().map(|part| match part {
            TemplatePart::Text(text) => text.clone(),
            TemplatePart::Variable(var) => format!("${var}"),
        }).collect::<Vec<String>>().join("")
    }

    fn render_request(&self, req: RestRequest) -> String {
        let RestRequest { headers, method, query, body, url, .. } = req; 
        let variables = self.render_variables(); 
        let headers = self.render_headers(headers);
        let method = self.render_method(method); 
        let query = self.render_query(query);
        let (body, output) = self.render_body(body);
        let url = self.render_url(url);

        format!("{variables}curl {url}{query}{method}{output}{headers}{body}")
    }
}


fn main() {
    let test_file = "../test_data/http_bin.http";
    let RestFormat { requests, variables, .. }= RestFormat::parse_file(test_file).unwrap(); 
  
    let renderer = CurlRenderer::new(Some(variables));

    for req in requests {
        let name = req.name.clone();
        let cmd = renderer.render_request(req);
        println!("{}", name.unwrap_or("Request".to_string()));
        println!("--------------");
        println!("{cmd}\n");
    }
}
