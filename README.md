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
use rest_parser::{RestFormat, RestFlavor, RestRequest};

fn main() {
    // From a file
    let _format = RestFormat::parse_file("../test_data/jetbrains.http").unwrap();

    // From a string
    let rest_data =  r#"
@HOST = http://httpbin.org
### SimpleGet
GET {{HOST}}/get HTTP/1.1"#;

    let format = RestFormat::parse(rest_data, RestFlavor::Jetbrains).unwrap();

    let host_var = format.variables.get("HOST");
    assert!(host_var == Some(&"http://httpbin.org".into()));
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
