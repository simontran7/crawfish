use crawfish::arg_parser;
use crawfish::driver;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let command = arg_parser::parse_args(&args);
    match command {
        Ok(arg_parser::Command::Build { path, emit }) => {
            driver::build(path, &emit);
        }
        Ok(arg_parser::Command::Run { path, emit }) => {
            driver::run(path, &emit);
        }
        Ok(arg_parser::Command::Check { path, emit }) => {
            driver::check(path, &emit);
        }
        Ok(arg_parser::Command::Help) => {
            let message = r#"crawfish compiler

Usage: crawfish [options] <arguments>

Arguments:
  build <file>.crw               build (AOT) the current file into an executable
  run <file>.crw                 build and run (JIT) the current file
  check <file>.crw               check the current file without producing an executable

Options:
  -h, --help                    print this message
  -v, --version                 print version information
  --emit=<kinds>                comma-separated intermediate representations to print
                                 ast, hir, mir, llvm-ir, dot (MIR control-flow graph, Graphviz DOT)
                                 (only printed for stages reached before the first error)"#;
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
