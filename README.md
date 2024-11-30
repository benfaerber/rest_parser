# rest_parser

Parse [VSCode `.rest` files](https://github.com/Huachao/vscode-restclient) and [Jetbrains `.http` files](https://www.jetbrains.com/help/idea/http-client-in-product-code-editor.html).

These are common files used to integrate API testing into IDEs.
There are multiple similar tools out there and it can be tedious to switch over.
This library was created with the hope that people will be able to parse these files and convert them into better alternatives.

This library could also be used for some pretty cool codegen. For example, converting a `.http` file into a `PHP` class.

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

### Supported
- Global Variables: `@HOST = https://httpbin.org`
- Splitting requests with optional names: `###` or `### GetRequest`
- Naming requests: `# @name JsonRequest`
- Parsing `Basic` and `Bearer` auth headers
- Parsing query parameters

### Unsupported
- Transforming responses with Javascript
- Special handling for certain requests `# @no-log`, `# @no-cookie-jar`, etc
- Dumping requests into a file