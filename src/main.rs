use std::fs;

use crawfish::arg_parser;
use crawfish::driver;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let command = arg_parser::parse_args(&args);
    match command {
        Ok(arg_parser::Command::Compile(path)) => {
            let filename = path.to_string_lossy().to_string();
            let source = match fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading file {}: {}", filename, e);
                    std::process::exit(1);
                }
            };
            driver::compile(&filename, &source);
        }
        Ok(arg_parser::Command::Help) => {
            let message = r#"crawfish compiler

Usage:
    crawfish <COMMAND> [OPTIONS] [ARGS]

Command:
    compile <file>.crw            compile the current file

Options:
    -h, --help                    print possible commands
    -v, --version                 print compiler version"#;
            println!("{message}");
        }
        Ok(arg_parser::Command::Version) => {
            let message = "crawfish 0.0.1";
            println!("{message}");
        }
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    }
}
