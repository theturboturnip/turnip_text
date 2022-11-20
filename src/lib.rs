mod lexer;
mod parser;

pub use parser::parse_simple_tokens;
pub use parser::ParseError;

#[cfg(test)]
mod tests;

/// A turnip-text Token, represented by groups of [lexer::SimpleToken]
///
/// TODO convert String to &'a str
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    /// Python code to evaluate, without any newlines, contained in the String
    Code(String),
    /// A Scope containing Tokens
    Scope(Vec<Token>),
    /// A raw scope containing a string that may have newlines
    RawScope(String),
    /// Inline text, without any newlines
    Text(String),
    /// Newline that is not contained in a [Token::RawScope].
    Newline,
    // TODO add doc-comment type that gets included in output latex?
}
