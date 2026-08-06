use crate::front_end::lexical_analysis::token::Token;

pub(crate) struct TokenDumper<'a> {
    source: &'a str,
}

impl<'a> TokenDumper<'a> {
    pub(crate) const fn new(source: &'a str) -> Self {
        Self { source }
    }

    pub(crate) fn pretty_print(&self, tokens: &[Token]) {
        println!("{:<6} {:<15} {:<20} Span", "Index", "Lexeme", "Kind");
        println!("{}", "-".repeat(60));
        for (i, token) in tokens.iter().enumerate() {
            let lexeme = &self.source[token.span().start() as usize..token.span().end() as usize];
            println!(
                "{:<6} {:<15} {:<20} [{}, {})",
                format!("#{}", i),
                format!("{:?}", lexeme),
                format!("{:?}", token.kind()),
                token.span().start(),
                token.span().end(),
            );
        }
    }
}
