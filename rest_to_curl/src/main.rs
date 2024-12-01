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

    fn render_body(&self, opt_body: Option<Body>) -> (String, String) {
        let mut save_to = None; 
        let rendered_body = opt_body.map(|body| match body {
            Body::Text(text) => text.render(&self.vars),
            Body::LoadFromFile { filepath, process_variables, .. } => self.load_body_from_file(filepath, process_variables),
            Body::SaveToFile { text, filepath } => {
                save_to = Some(filepath.render(&self.vars));
                text.render(&self.vars)
            },
        });
       
        let out_body = match rendered_body {
            Some(body_text) => {
                let encoded_body = body_text
                    .replace("\r\n", "")
                    .replace("\n", "")
                    .replace("\"", "\\\"");
                format!(" -d \"{}\"", encoded_body)
            },
            None => "".to_string()
        };

        let save_cmd = match save_to {
            Some(filename) => format!(" -o \"{filename}\""),
            None => "".to_string(),
        };

        (out_body, save_cmd)
    }

    fn render_headers(&self, headers: IndexMap<String, Template>) -> String {
        let all_headers = headers
            .iter()
            .map(|(k, v)| format!("-H \"{}: {}\"", k, v.render(&self.vars)))
            .collect::<Vec<String>>()
            .join(" ");
        format!(" {all_headers}")
    }

    fn render_url(&self, url: Template) -> String {
        url.render(&self.vars)
    }

    fn render_method(&self, method: Template) -> String {
        let method = method.render(&self.vars);
        format!(" -X {method}")
    }

    fn render_request(&self, req: RestRequest) -> String {
        let RestRequest { headers, method, query, body, url, .. } = req; 
        let headers = self.render_headers(headers);
        let method = self.render_method(method); 
        let query = self.render_query(query);
        let (body, output) = self.render_body(body);
        let url = self.render_url(url);
        
        format!("curl {url}{query}{method}{output}{headers}{body}")
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
