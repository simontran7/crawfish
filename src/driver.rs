use crate::common::context::CompilerContext;
use crate::front_end::lexical_analysis::token_tree_parser::TokenTreeParser;
use crate::front_end::lexical_analysis::tokenizer::Tokenizer;
use crate::front_end::semantic_analysis::hir_dumper::HirDumper;
use crate::front_end::semantic_analysis::semantic_analyzer::SemanticAnalyzer;
use crate::front_end::syntactic_analysis::ast_dumper::AstDumper;
use crate::front_end::syntactic_analysis::parser::Parser;

use crate::arg_parser::EmitKind;
use crate::back_end::link_driver::LinkDriver;
use crate::back_end::llvm_codegen::LlvmCodegen;
use crate::middle_end::dot_dumper::DotDumper;
use crate::middle_end::lowerer::MirLowerer;
use crate::middle_end::mir_dumper::MirDumper;
use crate::spinner::Spinner;

use std::io::IsTerminal;
use std::time::Instant;
use std::{fs, path::PathBuf};

use inkwell::OptimizationLevel;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{InitializationConfig, Target};

/// Prints a bold green `verb message` status line. Skips the color escapes
/// when stdout isn't a terminal.
fn print_status(verb: &str, message: &str) {
    if std::io::stdout().is_terminal() {
        println!("\x1b[1;34m{verb}\x1b[0m {message}");
    } else {
        println!("{verb} {message}");
    }
}

/// Builds `path`, printing any diagnostics to stderr, and returns the path
/// to the produced executable on success.
///
/// Every stage adds what it finds to [`CompilerContext::diagnostics`] and
/// keeps going, so a stage reports everything it found rather than bailing at
/// the first problem. The pipeline stops at the first stage boundary where an
/// error (as opposed to a warning) was recorded.
///
/// # Examples
///
/// ```rust,ignore
/// crawfish::driver::build("example.crw".into(), &[]);
/// ```
pub fn build(path: PathBuf, emit: &[EmitKind]) -> Option<PathBuf> {
    let start = Instant::now();
    let llvm_context = Context::create();
    let (unit, module) = compile_to_module(path, &llvm_context, emit)?;
    let executable_path = unit.path.with_extension("");
    let mut spinner = Spinner::start("Linking", unit.name.clone());
    let link_result = LinkDriver::new(&module, &executable_path).link();
    spinner.stop();
    unit.ctx.diagnostics.render(&unit.filename, &unit.source);
    match link_result {
        Ok(()) => {
            print_status(
                "Compiled and linked",
                &format!("in {:.2}s", start.elapsed().as_secs_f64()),
            );
            Some(executable_path)
        }
        Err(e) => {
            eprintln!("Error producing executable: {e}");
            None
        }
    }
}

/// Compiles `path` and JIT-executes the result directly in this process,
/// forwarding `main`'s return value as the process exit code.
///
/// `run` is for immediate feedback, so unlike [`build`] it never touches
/// disk: no executable file, no linker invocation, and no OS-level
/// first-launch security check paid on every call — those turned out to
/// dominate wall-clock time far more than the compiler itself.
pub fn run(path: PathBuf, emit: &[EmitKind]) {
    let start = Instant::now();
    let llvm_context = Context::create();
    let Some((unit, module)) = compile_to_module(path, &llvm_context, emit) else {
        std::process::exit(1);
    };
    unit.ctx.diagnostics.render(&unit.filename, &unit.source);
    print_status(
        "Compiled",
        &format!("in {:.2}s", start.elapsed().as_secs_f64()),
    );
    print_status("Running", &format!("{}.main", unit.name));

    Target::initialize_native(&InitializationConfig::default())
        .expect("failed to initialize native target for JIT execution");
    let execution_engine = module
        .create_jit_execution_engine(OptimizationLevel::None)
        .unwrap_or_else(|e| {
            eprintln!("Error creating JIT execution engine: {e}");
            std::process::exit(1);
        });
    let exit_code = unsafe {
        let main_function = execution_engine
            .get_function::<unsafe extern "C" fn() -> i32>("main")
            .unwrap_or_else(|e| {
                eprintln!("Error locating `main`: {e:?}");
                std::process::exit(1);
            });
        main_function.call()
    };
    std::process::exit(exit_code);
}

/// Checks `path` for diagnostics without producing an executable.
pub fn check(path: PathBuf, emit: &[EmitKind]) {
    let start = Instant::now();
    let llvm_context = Context::create();
    let Some((unit, _module)) = compile_to_module(path, &llvm_context, emit) else {
        return;
    };
    unit.ctx.diagnostics.render(&unit.filename, &unit.source);
    print_status(
        "Checked",
        &format!("in {:.2}s", start.elapsed().as_secs_f64()),
    );
}

/// Everything [`compile_to_module`] produces besides the LLVM [`Module`]
/// itself: the state needed to render diagnostics.
struct CompileUnit {
    ctx: CompilerContext,
    path: PathBuf,
    filename: String,
    source: String,
    /// The file stem (e.g. `hello_world` for `hello_world.crw`), used as
    /// the program name in status lines like `Running hello_world.main`.
    name: String,
}

/// Runs the whole pipeline on `path` — lexical analysis through LLVM codegen
/// — returning a [`CompileUnit`] alongside the resulting [`Module`]. Prints
/// the intermediate representations named in `emit` (`--emit=ast,hir,mir,llvm-ir,dot`)
/// as each becomes available.
/// `llvm_context` is supplied by the caller since the returned `Module`
/// borrows from it. Returns `None` — having already rendered diagnostics —
/// if any stage reported an error; exits directly if `path` itself couldn't
/// be read.
fn compile_to_module<'ctx>(
    path: PathBuf,
    llvm_context: &'ctx Context,
    emit: &[EmitKind],
) -> Option<(CompileUnit, Module<'ctx>)> {
    let mut ctx = CompilerContext::new();
    let name = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let mut spinner = Spinner::start("Compiling", name.clone());

    // source reading
    let filename = path.to_string_lossy().to_string();
    let source = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file {}: {}", filename, e);
            std::process::exit(1);
        }
    };

    // lexical analysis
    let mut tokenizer = Tokenizer::new(&source, &mut ctx);
    let tokens = tokenizer.tokenize();
    let token_trees = TokenTreeParser::new(tokens, &ctx).parse();
    if ctx.diagnostics.has_errors() {
        spinner.stop();
        ctx.diagnostics.render(&filename, &source);
        return None;
    }

    // syntactic analysis
    let ast = Parser::new(&source, &token_trees, &ctx).parse();
    if ctx.diagnostics.has_errors() {
        spinner.stop();
        ctx.diagnostics.render(&filename, &source);
        return None;
    }
    if emit.contains(&EmitKind::Ast) {
        spinner.stop();
        match AstDumper::new(&ast, &ctx).dump() {
            Ok(output) => println!("{}", output),
            Err(e) => eprintln!("Error dumping AST: {}", e),
        };
    }

    // semantic analysis
    let hir = SemanticAnalyzer::new(&ast, &mut ctx).analyze();
    if ctx.diagnostics.has_errors() {
        spinner.stop();
        ctx.diagnostics.render(&filename, &source);
        return None;
    }
    if emit.contains(&EmitKind::Hir) {
        spinner.stop();
        match HirDumper::new(&hir, &ctx).dump() {
            Ok(output) => print!("{}", output),
            Err(e) => eprintln!("Error dumping HIR: {}", e),
        };
    }

    // mir lowering
    let mir = MirLowerer::new(&hir, &ctx).lower();
    if ctx.diagnostics.has_errors() {
        spinner.stop();
        ctx.diagnostics.render(&filename, &source);
        return None;
    }
    if emit.contains(&EmitKind::Mir) {
        spinner.stop();
        match MirDumper::new(&mir, &ctx).dump() {
            Ok(output) => print!("{}", output),
            Err(e) => eprintln!("Error dumping MIR: {}", e),
        };
    }
    if emit.contains(&EmitKind::Dot) {
        spinner.stop();
        match DotDumper::new(&mir, &ctx).dump() {
            Ok(output) => print!("{}", output),
            Err(e) => eprintln!("Error dumping DOT: {}", e),
        };
    }

    // llvm ir generation
    let module = LlvmCodegen::new(&mir, &ctx, llvm_context, &filename).compile();
    spinner.stop();
    if emit.contains(&EmitKind::LlvmIr) {
        println!("{}", module.print_to_string().to_string());
    }

    Some((
        CompileUnit {
            ctx,
            path,
            filename,
            source,
            name,
        },
        module,
    ))
}
