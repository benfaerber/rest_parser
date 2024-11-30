use rest_parser::{RestFormat, RestRequest, RestVariables};

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


fn main() {
    let format = RestFormat::parse_file("../test_data/jetbrains.http").unwrap();

    for (index, request) in format.requests.iter().enumerate() {
        print_request(request, index + 1);
    }
    
    println!("\n\n");
    let vars: RestVariables = format.variables;
    print_variables(&vars);
}
