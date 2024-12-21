# rest_parser

Parse [VSCode `.rest` files](https://github.com/Huachao/vscode-restclient) and [Jetbrains `.http` files](https://www.jetbrains.com/help/idea/http-client-in-product-code-editor.html).

These are common files used to integrate API testing into IDEs.
It is not for processing REST files and sending requests only for parsing and turning the files into structured data.
This library was created with the hope that people will be able to parse these files and convert them into alternate formats (HURL, Slumber, Python Requests, etc).
There are multiple similar file formats out there and it can be tedious to switch over.

Check out [rest_to_curl](https://github.com/benfaerber/rest_parser/blob/master/rest_to_curl/src/main.rs) for an example usecase!

## Getting Started:

This library exports the `RestFormat` struct which is used to parse:
```rust
fn main() {
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
```

## Features:
Not all features have been ported over, mostly because they are security risks and/or super niche.
One example is [running Javascript to transform responses](https://www.jetbrains.com/help/idea/exploring-http-syntax.html#per_request_variables) in the Jetbrains flavor.

Note that this is just a parser, so by supported I mean the listed feature is able to be parsed. Its up the implementor to setup the behavior for each feature.
Checkout the [rest_to_curl](https://github.com/benfaerber/rest_parser/blob/master/rest_to_curl/src/main.rs) to see this library in action.

### Supported
- Global Variables: `@HOST = https://httpbin.org`
- Splitting requests with optional names: `###` or `### GetRequest`
- Naming requests: `# @name JsonRequest`
- Parsing `Basic` and `Bearer` auth headers
- Parsing query parameters
- Loading request body from a file
- Saving response body to a file
- Special handling for certain requests `# @no-log`, `# @no-cookie-jar`, etc

### Unsupported
- Transforming responses with Javascript
