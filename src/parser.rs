use crate::lexer::{LexPosn, SimpleToken};
use lexer_rs::{PosnInCharStream, StreamCharSpan, UserPosn};
use thiserror::Error;

/// A parsed token, extracted from groups of [lexer::SimpleToken]
///
/// TODO convert String to &'a str
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseToken {
    /// Python code to evaluate, may contain newlines
    Code(String),
    /// A Scope containing further ParseTokens
    Scope(Vec<ParseToken>),
    /// A raw scope containing a string that may have newlines
    RawScope(String),
    /// Text without any newlines
    Text(String),
    /// Newline that is not contained in [ParseToken::RawScope] or [ParseToken::Code].
    Newline,
    // TODO add doc-comment type that gets included in output latex?
}

/// Helper struct representing the position of a character in a file, as both:
/// - Byte offset of the start of the UTF-8 code point
/// - (line, column) integers for display purposes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsePosn {
    pub byte_ofs: usize,
    pub line: usize,
    pub column: usize,
}
impl From<LexPosn> for ParsePosn {
    fn from(p: LexPosn) -> Self {
        ParsePosn {
            byte_ofs: p.byte_ofs(),
            line: p.line(),
            column: p.column(),
        }
    }
}

/// Helper struct representing a span of characters between `start` (inclusive) and `end` (exclusive) in a file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseSpan {
    pub start: ParsePosn,
    pub end: ParsePosn,
}

/// Enumeration of all possible parsing errors
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseError {
    #[error("Code close encountered in text mode")]
    CodeCloseInText(ParseSpan),
    #[error("Scope close encountered with no matching scope open")]
    ScopeCloseOutsideScope(ParseSpan),
    #[error("Scope close with {n_hashes} hashes encountered when closest scope open has {expected_closing_hashes}")]
    MismatchingScopeClose {
        n_hashes: usize,
        expected_closing_hashes: usize,
        scope_open_span: ParseSpan,
        scope_close_span: ParseSpan,
    },
    #[error("File ended inside code block")]
    EndedInsideCode { code_start: ParseSpan },
    #[error("File ended inside raw scope")]
    EndedInsideRawScope { raw_scope_start: ParseSpan },
    #[error("File ended inside scope")]
    EndedInsideScope { scope_start: ParseSpan },
}

/// Parses a stream of [SimpleToken] into a vector of [Token].
///
/// If a parse error is encountered, the parsed vector up to that point is still returned.
pub fn parse_simple_tokens<P>(
    data: &str,
    token_stream: Box<dyn Iterator<Item = SimpleToken<P>>>,
) -> Result<Vec<ParseToken>, ParseError>
where
    P: PosnInCharStream + Into<ParsePosn>,
{
    let mut parser = ParserState::new(data);
    let res: Result<(), ParseError> = token_stream
        .map(|stok| parser.parse_simple_token(stok))
        .collect();
    res?;
    parser.finalize()
}

// TODO block comments

struct ParserScope {
    scope_start: ParseSpan,
    expected_closing_hashes: usize,
    tokens: Vec<ParseToken>,
}
enum ParserInlineMode {
    Comment,
    InlineText(String),
    InlineCode {
        code_start: ParseSpan,
        expected_closing_hashes: usize,
        content: String,
    },
    RawScope {
        raw_scope_start: ParseSpan,
        expected_closing_hashes: usize,
        content: String,
    },
}
impl Default for ParserInlineMode {
    fn default() -> Self {
        ParserInlineMode::InlineText("".into())
    }
}
impl From<ParserInlineMode> for Option<ParseToken> {
    fn from(mode: ParserInlineMode) -> Self {
        match mode {
            ParserInlineMode::Comment => None,
            ParserInlineMode::InlineText(text) => {
                if text.is_empty() {
                    None
                } else {
                    Some(ParseToken::Text(text))
                }
            }
            ParserInlineMode::InlineCode { content, .. } => Some(ParseToken::Code(content)),
            ParserInlineMode::RawScope { content, .. } => Some(ParseToken::RawScope(content)),
        }
    }
}
#[derive(Debug, Clone, Copy)]
enum ParserAction {
    EndTokenAndNewlineAndStartText,
    EndTokenAndStartText,
    EndTokenAndStartComment,
    EndTokenAndStartCode(ParseSpan, usize),
    EndTokenAndStartRawScope(ParseSpan, usize),
    EndTokenAndPushNewScope(ParseSpan, usize),
    EndTokenAndPopScope,
    NoAction,
}
impl ParserAction {
    fn should_add_newline_token(&self) -> bool {
        match self {
            Self::EndTokenAndNewlineAndStartText => true,
            _ => false,
        }
    }
    fn should_push_scope(&self) -> Option<(ParseSpan, usize)> {
        match self {
            Self::EndTokenAndPushNewScope(span, hashes) => Some((*span, *hashes)),
            _ => None,
        }
    }
    fn should_pop_scope(&self) -> bool {
        match self {
            Self::EndTokenAndPopScope => true,
            _ => false,
        }
    }
    fn should_transition_to_new_mode(&self) -> Option<ParserInlineMode> {
        use ParserAction::*;
        match *self {
            EndTokenAndNewlineAndStartText
            | EndTokenAndStartText
            | EndTokenAndPushNewScope(..)
            | EndTokenAndPopScope => Some(ParserInlineMode::InlineText("".into())),

            EndTokenAndStartComment => Some(ParserInlineMode::Comment),
            EndTokenAndStartCode(code_start, closing_hashes) => {
                Some(ParserInlineMode::InlineCode {
                    code_start,
                    expected_closing_hashes: closing_hashes,
                    content: "".into(),
                })
            }
            EndTokenAndStartRawScope(raw_scope_start, closing_hashes) => {
                Some(ParserInlineMode::RawScope {
                    raw_scope_start,
                    expected_closing_hashes: closing_hashes,
                    content: "".into(),
                })
            }

            NoAction => None,
        }
    }
}
struct ParserState<'a> {
    pub scope_stack: Vec<ParserScope>,
    pub inline_mode: ParserInlineMode,
    pub tokens: Vec<ParseToken>,
    data: &'a str,
}
impl<'a> ParserState<'a> {
    pub fn new(data: &'a str) -> Self {
        Self {
            scope_stack: vec![],
            inline_mode: Default::default(),
            tokens: vec![],
            data,
        }
    }
    pub fn parser_span<P>(&self, l: &StreamCharSpan<P>) -> ParseSpan
    where
        P: PosnInCharStream + Into<ParsePosn>,
    {
        ParseSpan {
            start: (*l.start()).into(),
            end: (*l.end()).into(),
        }
    }
    fn parse_simple_token<P>(&mut self, stok: SimpleToken<P>) -> Result<(), ParseError>
    where
        P: PosnInCharStream + Into<ParsePosn>,
    {
        use ParserAction::*;
        use SimpleToken::*;

        let current_scope = self.scope_stack.last();

        let action = match &mut self.inline_mode {
            ParserInlineMode::InlineText(text) => match stok {
                // State transitions
                Newline(_) => EndTokenAndNewlineAndStartText,
                Hashes(_, _) => EndTokenAndStartComment,
                CodeOpen(span, n) => EndTokenAndStartCode(self.parser_span(&span), n),
                RawScopeOpen(span, n) => EndTokenAndStartRawScope(self.parser_span(&span), n),

                // Handle scopes
                ScopeOpen(span, n) => EndTokenAndPushNewScope(self.parser_span(&span), n),
                ScopeClose(span, n_hashes) => match current_scope {
                    Some(ParserScope {
                        expected_closing_hashes,
                        ..
                    }) if n_hashes == *expected_closing_hashes => EndTokenAndPopScope,
                    Some(ParserScope {
                        expected_closing_hashes,
                        scope_start,
                        ..
                    }) => Err(ParseError::MismatchingScopeClose {
                        n_hashes,
                        expected_closing_hashes: *expected_closing_hashes,
                        scope_open_span: *scope_start,
                        scope_close_span: self.parser_span(&span),
                    })?,
                    None => Err(ParseError::ScopeCloseOutsideScope(self.parser_span(&span)))?,
                },

                // Handle invalid code close
                CodeClose(span, _) => Err(ParseError::CodeCloseInText(self.parser_span(&span)))?,

                // Handle valid text
                OtherText(_) | Backslash(_) => {
                    text.push_str(stok.stringify(self.data));
                    NoAction
                }
                Escaped(_, escapee) => {
                    text.push_str(escapee.stringify());
                    NoAction
                }
            },
            ParserInlineMode::InlineCode {
                expected_closing_hashes: closing_hashes,
                content,
                ..
            } => match stok {
                // Close inline code with a token using the same amount of hashes as the opener
                CodeClose(_, n) if n == *closing_hashes => EndTokenAndStartText,

                // All other tokens treated as python code
                _ => {
                    content.push_str(stok.stringify(self.data));
                    NoAction
                }
            },
            ParserInlineMode::Comment => match stok {
                // Finish the comment with a newline
                Newline(_) => EndTokenAndNewlineAndStartText,
                // All other tokens ignored
                _ => NoAction,
            },
            ParserInlineMode::RawScope {
                expected_closing_hashes,
                content,
                ..
            } => match stok {
                // Close inline code with a token using the same amount of hashes as the opener
                ScopeClose(_, n) if n == *expected_closing_hashes => EndTokenAndStartText,
                // If we hit a newline, pass a consistent \n
                Newline(_) => {
                    content.push_str("\n");
                    NoAction
                }
                // All other tokens taken exactly as from the original text
                _ => {
                    content.push_str(stok.stringify(self.data));
                    NoAction
                }
            },
        };

        self.execute_action(action)?;
        Ok(())
    }

    fn execute_action(&mut self, action: ParserAction) -> Result<(), ParseError> {
        if let Some(new_inline_mode) = action.should_transition_to_new_mode() {
            let old_mode = std::mem::replace(&mut self.inline_mode, new_inline_mode);
            if let Some(token) = old_mode.into() {
                self.find_next_token_stack().push(token);
            }
        }
        if action.should_add_newline_token() {
            // We have to repeat this find_next_token_stack :(
            self.find_next_token_stack().push(ParseToken::Newline);
        }
        if action.should_pop_scope() {
            match self.scope_stack.pop() {
                Some(scope) => self
                    .find_next_token_stack()
                    .push(ParseToken::Scope(scope.tokens)),
                None => panic!(
                    "Executed action {:?} which pops scope with no scopes on the stack",
                    action
                ),
            }
        }
        if let Some((scope_start, expected_closing_hashes)) = action.should_push_scope() {
            assert!(
                !action.should_pop_scope(),
                "A ParserAction shouldn't try to push and pop scopes at the same time"
            );
            self.scope_stack.push(ParserScope {
                scope_start,
                expected_closing_hashes,
                tokens: vec![],
            })
        }

        Ok(())
    }

    fn find_next_token_stack<'b>(&'b mut self) -> &'b mut Vec<ParseToken> {
        self.scope_stack
            .last_mut()
            .map_or(&mut self.tokens, |scope| &mut scope.tokens)
    }

    pub fn finalize(mut self) -> Result<Vec<ParseToken>, ParseError> {
        match self.inline_mode {
            ParserInlineMode::Comment => {}
            // If we have text pending, put it into self.tokens
            ParserInlineMode::InlineText(_) => {
                self.execute_action(ParserAction::EndTokenAndStartText)?
            }
            // If we're in code or raw scope mode, something went wrong
            ParserInlineMode::InlineCode { code_start, .. } => {
                Err(ParseError::EndedInsideCode { code_start })?
            }
            ParserInlineMode::RawScope {
                raw_scope_start, ..
            } => Err(ParseError::EndedInsideRawScope { raw_scope_start })?,
        };
        match self.scope_stack.last() {
            Some(ParserScope { scope_start, .. }) => Err(ParseError::EndedInsideScope {
                scope_start: *scope_start,
            })?,
            None => {}
        };
        Ok(self.tokens)
    }
}
