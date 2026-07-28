use crawfish::arg_parser;
use crawfish::driver;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let command = arg_parser::parse_args(&args);
    match command {
        Ok(arg_parser::Command::Compile(path)) => {
            driver::compile(path);
        }
        Ok(arg_parser::Command::Run(path)) => {
            driver::run(path);
        }
        Ok(arg_parser::Command::Check(path)) => {
            driver::check(path);
        }
        Ok(arg_parser::Command::Help) => {
            let message = r#"crawfish compiler

Usage: crawfish [options] <arguments>

Arguments:
  compile <file>.crw            compile the current file
  run <file>.crw                compile and run the current file
  check <file>.crw              check the current file without producing an executable

Options:
  -h, --help                    print this message
  -v, --version                 print version information"#;
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
