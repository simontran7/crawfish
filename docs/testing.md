# Testing

The parser is tested using the [snapshot testing](https://www.cs.cornell.edu/~asampson/blog/turnt.html) technique using [insta](https://github.com/mitsuhiko/insta).

1. Create an input file `.crw` in `lexical_analysis/inputs/`, `syntactic_analysis/inputs/`, or `semantic_analysis/inputs/`.

2. Each phase has a single test that globs over all inputs (written once):
```rust
#[cfg(test)]
mod tests {
    use super::Parser;
    use crate::common::string_interner::StringInterner;
    use crate::front_end::lexical_analysis::token_tree_parser::TokenTreeParser;
    use crate::front_end::lexical_analysis::tokenizer::Tokenizer;
    use crate::front_end::syntactic_analysis::ast_dumper::AstDumper;

    #[test]
    fn test_parser_output() {
        insta::glob!("inputs/**/*.crw", |path| {
            let source = std::fs::read_to_string(path).unwrap();
            let filename = path.file_name().unwrap().to_str().unwrap();

            let mut string_interner = StringInterner::new();

            let tokens = Tokenizer::new(&source, &mut string_interner).tokenize();

            let token_trees = TokenTreeParser::new(tokens).parse().unwrap();

            let ast = match Parser::new(&source, &token_trees, &string_interner).parse() {
                Ok(ast) => ast,
                Err(diagnostics) => {
                    let output = diagnostics
                        .iter()
                        .map(|d| format!("{d:?}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    insta::assert_snapshot!(filename, output);
                    return;
                }
            };

            let output = AstDumper::new(&ast, &string_interner).dump().unwrap();
            insta::assert_snapshot!(filename, output);
        });
    }
}
```

3. Run the test (it'll fail because no snapshot exists yet).

4. Review the snapshot using `cargo insta review` to make sure the output looks correct. If it does, accept it. The subsequent test run should pass because the output matches the accepted `.snap`.

5. Commit the `.snap` file to git. This is what makes it a regression test going forward.

