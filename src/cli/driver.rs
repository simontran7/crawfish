use crate::common::context::CompilerContext;
use crate::front_end::lexical_analysis::token_tree_parser::TokenTreeParser;
use crate::front_end::lexical_analysis::tokenizer::Tokenizer;
use crate::front_end::semantic_analysis::hir_dumper::HirDumper;
use crate::front_end::semantic_analysis::semantic_analyzer::SemanticAnalyzer;
use crate::front_end::syntactic_analysis::ast_dumper::AstDumper;
use crate::front_end::syntactic_analysis::parser::Parser;

use crate::back_end::link_driver::LinkDriver;
use crate::back_end::llvm_codegen::LlvmCodegen;
use crate::cli::arg_parser::EmitKind;
use crate::cli::spinner::Spinner;
use crate::middle_end::dot_dumper::DotDumper;
use crate::middle_end::lowerer::MirLowerer;
use crate::middle_end::mir_dumper::MirDumper;

use std::io::IsTerminal;
use std::time::Instant;
use std::{fs, path::PathBuf};

use inkwell::OptimizationLevel;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{InitializationConfig, Target};

pub fn build(path: PathBuf, emit: &[EmitKind]) -> Option<PathBuf> {
    let start = Instant::now();
    let llvm_context = Context::create();
    let (unit, module) = compile_to_module(path, &llvm_context, emit)?;
    let executable_path = unit.path.with_extension("");
    let mut spinner = Spinner::start("Linking", unit.file_stem.clone());
    let link_result = LinkDriver::new(&module, &executable_path).link();
    spinner.stop();
    unit.ctx
        .diagnostics
        .render(&unit.display_path, &unit.source);
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

pub fn run(path: PathBuf, emit: &[EmitKind]) {
    let start = Instant::now();
    let llvm_context = Context::create();
    let Some((unit, module)) = compile_to_module(path, &llvm_context, emit) else {
        std::process::exit(1);
    };
    unit.ctx
        .diagnostics
        .render(&unit.display_path, &unit.source);
    print_status(
        "Compiled",
        &format!("in {:.2}s", start.elapsed().as_secs_f64()),
    );
    print_status("Running", &format!("{}.main", unit.file_stem));

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

pub fn check(path: PathBuf, emit: &[EmitKind]) {
    let start = Instant::now();
    let llvm_context = Context::create();
    let Some((unit, _module)) = compile_to_module(path, &llvm_context, emit) else {
        return;
    };
    unit.ctx
        .diagnostics
        .render(&unit.display_path, &unit.source);
    print_status(
        "Checked",
        &format!("in {:.2}s", start.elapsed().as_secs_f64()),
    );
}

struct CompileUnit {
    ctx: CompilerContext,
    path: PathBuf,
    display_path: String,
    source: String,
    file_stem: String,
}

fn compile_to_module<'ctx>(
    path: PathBuf,
    llvm_context: &'ctx Context,
    emit: &[EmitKind],
) -> Option<(CompileUnit, Module<'ctx>)> {
    let mut ctx = CompilerContext::new();
    let file_stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let mut spinner = Spinner::start("Compiling", file_stem.clone());

    // source reading
    let display_path = path.to_string_lossy().to_string();
    let source = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file {}: {}", display_path, e);
            std::process::exit(1);
        }
    };

    // lexical analysis
    let tokens = Tokenizer::new(&source, &mut ctx).collect::<Vec<_>>();
    let token_trees = TokenTreeParser::new(tokens, &ctx).parse();
    if ctx.diagnostics.has_errors() {
        spinner.stop();
        ctx.diagnostics.render(&display_path, &source);
        return None;
    }

    // syntactic analysis
    let ast = Parser::new(&source, &token_trees, &ctx).parse();
    if ctx.diagnostics.has_errors() {
        spinner.stop();
        ctx.diagnostics.render(&display_path, &source);
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
        ctx.diagnostics.render(&display_path, &source);
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
        ctx.diagnostics.render(&display_path, &source);
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
    let module = LlvmCodegen::new(&mir, &ctx, llvm_context, &display_path).compile();
    spinner.stop();
    if emit.contains(&EmitKind::LlvmIr) {
        println!("{}", module.print_to_string().to_string());
    }

    Some((
        CompileUnit {
            ctx,
            path,
            display_path,
            source,
            file_stem,
        },
        module,
    ))
}

fn print_status(verb: &str, message: &str) {
    if std::io::stdout().is_terminal() {
        println!("\x1b[1;34m{verb}\x1b[0m {message}");
    } else {
        println!("{verb} {message}");
    }
}
