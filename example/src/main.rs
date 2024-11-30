use rest_parser::{RestFormat, RestRequest, RestVariables, RestFlavor};

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
@HOST = http://httpbin.org
### SimpleGet
GET {{HOST}}/get HTTP/1.1"#;

    let RestFormat { requests, variables, flavor } = RestFormat::parse(
        rest_data,
        // Normally, the flavor is determined by the file extension.
        RestFlavor::Jetbrains
    ).unwrap();
    
    let host_var = variables.get("HOST");
    assert_eq!(host_var, Some(&"http://httpbin.org".into()));

    let method = &requests.first().unwrap().method;
    assert_eq!(method, "GET");

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
