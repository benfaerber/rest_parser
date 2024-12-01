use rest_parser::{RestFormat, RestRequest, RestVariables, IndexMap, Template, Body};
use std::fs;

struct CurlRenderer {
    vars: RestVariables,
}
 
impl CurlRenderer {
    pub fn new(variables: Option<RestVariables>) -> Self {
        let vars = variables.unwrap_or(RestVariables::new());
        Self { vars }
    }


    fn render_query(&self, query: IndexMap<String, Template>) -> String {
        let params = query.iter()
            .map(|(k, v)| format!("{}={}", k, v.render(&self.vars)))
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
            Template::new(&raw).render(&self.vars)
        } else {
            raw
        }
    }

    fn render_body(&self, opt_body: Option<Body>) -> String {
        let map_body = opt_body.map(|body| match body {
            Body::Text(t) => t.render(&self.vars),
            Body::LoadFromFile { filepath, process_variables, .. } => self.load_body_from_file(filepath, process_variables),
            Body::SaveToFile { text, .. } => text.render(&self.vars),
        });
        
        let body_cmd = match map_body {
            Some(b) => {
                let encoded_body = b
                    .replace("\r\n", "")
                    .replace("\"", "\\\"");
                format!(" -d \"{}\"", encoded_body)
            },
            None => "".to_string()
        };

        body_cmd
    }

    fn render_headers(&self, headers: IndexMap<String, Template>) -> String {
        headers
            .iter()
            .map(|(k, v)| format!("-H \"{}: {}\"", k, v.render(&self.vars)))
            .collect::<Vec<String>>()
            .join(" ")
    }

    fn render_url(&self, url: Template) -> String {
        url.render(&self.vars)
    }

    fn render_method(&self, method: Template) -> String {
        let method = method.render(&self.vars);
        format!("-X {method}")
    }

    fn render_request(&self, req: RestRequest) -> String {
        let headers = self.render_headers(req.headers);
        let method = self.render_method(req.method); 
        let query = self.render_query(req.query);
        let body_cmd = self.render_body(req.body);
        let url = self.render_url(req.url);

        let cmd = format!("curl {url}{query} {method} {headers}{body_cmd}");
        cmd
    }
}


fn main() {
    let test_file = "../test_data/http_bin.http";
    let rest = RestFormat::parse_file(test_file).unwrap(); 
  
    let renderer = CurlRenderer::new(
        Some(rest.variables.clone())
    );

    for req in rest.requests {
        let name = req.name.clone();
        let cmd = renderer.render_request(req);
        println!("{}", name.unwrap_or("Request".to_string()));
        println!("--------------");
        println!("{cmd}\n");
    }
}
