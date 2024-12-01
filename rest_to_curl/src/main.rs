use rest_parser::{RestFormat, RestRequest, RestVariables, IndexMap, Template};

fn render_query(query: IndexMap<String, Template>, vars: &RestVariables) -> String {
    query.iter()
        .map(|(k, v)| format!("{}={}", k, v.render(&vars)))
        .collect::<Vec<String>>()
        .join("&")
}

fn request_to_curl(req: &RestRequest, variables: Option<RestVariables>) -> String {
    let vars = variables
        .unwrap_or(RestVariables::new()); 

    let headers: String = req.headers
        .iter()
        .map(|(k, v)| format!("-H {}: {}", k, v.render(&vars)))
        .collect::<Vec<String>>()
        .join(" ");

    let method = format!("-X {}", req.method.render(&vars));
    let query = render_query(req.query.clone(), &vars);
    let query = if query.is_empty() {
        "".to_string()
    } else {
        format!("?{query}")
    };

        
    let url = req.url.render(&vars);
    let cmd = format!("curl {url}{query} {method} {headers}");
    cmd
}


fn main() {
    let test_file = "../test_data/http_bin.http";
    let rest = RestFormat::parse_file(test_file).unwrap(); 
   
    for req in rest.requests {
        let vars = Some(rest.variables.clone());
        let cmd = request_to_curl(&req, vars);
        println!("{cmd}\n");
    }
}
