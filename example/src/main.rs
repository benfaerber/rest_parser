use rest_parser::{template::TemplatePart, RestFlavor, RestFormat, RestRequest, RestVariables};

fn print_request(request: &RestRequest, number: usize) {
    println!("Request Number {number}"); 
    println!("-----------------------"); 
    println!("{:#?}\n", request);
}

fn print_variables(vars: &RestVariables) {
    for (name, value) in vars {
        println!("Variable '{name}' is '{value}'");
    }
} 

fn main_from_docs() {
    // From a file
    let _format = RestFormat::parse_file("../test_data/jetbrains.http").unwrap();

    // From a string 
    let rest_data =  r#"
@HOST = https://httpbin.org
### SimpleGet
GET {{HOST}}/get HTTP/1.1"#;

    let RestFormat { requests, variables, flavor } = RestFormat::parse(
        rest_data,
        // Normally, the flavor is determined by the file extension.
        RestFlavor::Jetbrains
    ).unwrap();
    
    let host_var = variables.get("HOST").unwrap();
    assert_eq!(host_var.to_string(), "https://httpbin.org");

    let req = requests.first().unwrap();
    assert_eq!(req.method.raw, "GET");
    assert_eq!(req.url.parts.first().unwrap(), &TemplatePart::var("HOST"));
    assert_eq!(req.url.parts.get(1).unwrap(), &TemplatePart::text("/get"));

    // Render the variables using the template
    let rendered_url = req.url.render(&variables);
    assert_eq!(rendered_url, "https://httpbin.org/get");

    assert_eq!(flavor, RestFlavor::Jetbrains);
}

fn main() {
    let format = RestFormat::parse_file("../test_data/jetbrains.http").unwrap();

    for (index, request) in format.requests.iter().enumerate() {
        print_request(request, index + 1);
    }
    
    let vars: RestVariables = format.variables;
    print_variables(&vars);

    main_from_docs();
}
