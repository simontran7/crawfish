use crate::common::string_interner::StringInterner;
use crate::front_end::lexical_analysis::token_tree_parser::TokenTreeParser;
use crate::front_end::lexical_analysis::tokenizer::Tokenizer;
use crate::front_end::semantic_analysis::hir_dumper::HirDumper;
use crate::front_end::semantic_analysis::semantic_analyzer::SemanticAnalyzer;
use crate::front_end::semantic_analysis::types::TypeInterner;
use crate::front_end::syntactic_analysis::ast_dumper::AstDumper;
use crate::front_end::syntactic_analysis::parser::Parser;

/// Compiles `source` named `filename`.
pub fn compile(filename: &str, source: &str) {
    // interners
    let mut string_interner = StringInterner::new();
    let mut type_interner = TypeInterner::new();

    // lexical analysis
    let mut tokenizer = Tokenizer::new(source, &mut string_interner);
    let tokens = tokenizer.tokenize();
    let token_trees = match TokenTreeParser::new(tokens).parse() {
        Ok(trees) => trees,
        Err(diagnostics) => {
            for d in &diagnostics {
                d.report(filename, source);
            }
            return;
        }
    };

    // syntactic analysis
    let ast = match Parser::new(source, &token_trees, &string_interner).parse() {
        Ok(ast) => ast,
        Err(diagnostics) => {
            for d in &diagnostics {
                d.report(filename, source);
            }
            return;
        }
    };
    #[cfg(debug_assertions)]
    match AstDumper::new(&ast, &string_interner).dump() {
        Ok(output) => println!("{}", output),
        Err(e) => eprintln!("Error dumping AST: {}", e),
    };

    // semantic analysis
    let hir = match SemanticAnalyzer::new(&ast, &string_interner, &mut type_interner).analyze() {
        Ok(result) => result,
        Err(diagnostics) => {
            for d in &diagnostics {
                d.report(filename, source);
            }
            return;
        }
    };
    #[cfg(debug_assertions)]
    match HirDumper::new(&hir, &type_interner, &string_interner).dump() {
        Ok(output) => print!("{}", output),
        Err(e) => eprintln!("Error dumping HIR: {}", e),
    };
}

pub fn check() {
    todo!()
}
