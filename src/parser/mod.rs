use lexer_rs::PosnInCharStream;

use self::lexer::SimpleToken;

mod lexer;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    NewlineInCode,
    CodeCloseInText,
    ScopeCloseOutsideScope,
    MismatchingScopeClose(usize),
    EndedInsideCode,
    EndedInsideRawScope,
    EndedInsideScope,
}

/// Parses a stream of [SimpleToken] into a vector of [Token].
///
/// If a parse error is encountered, the parsed vector up to that point is still returned.
pub fn parse_simple_tokens<P>(
    data: &str,
    token_stream: Box<dyn Iterator<Item = SimpleToken<P>>>,
) -> Result<Vec<Token>, ParseError>
where
    P: PosnInCharStream,
{
    let mut parser = ParserState::new();
    let res: Result<(), ParseError> = token_stream
        .map(|stok| parser.parse_simple_token(data, stok))
        .collect();
    res?;
    parser.finalize()
}

// TODO block comments

struct ParserScope {
    closing_hashes: usize,
    tokens: Vec<Token>,
}
enum ParserInlineMode {
    Comment,
    InlineText(String),
    InlineCode {
        closing_hashes: usize,
        content: String,
    },
    RawScope {
        closing_hashes: usize,
        content: String,
    },
}
impl Default for ParserInlineMode {
    fn default() -> Self {
        ParserInlineMode::InlineText("".into())
    }
}
impl From<ParserInlineMode> for Option<Token> {
    fn from(mode: ParserInlineMode) -> Self {
        match mode {
            ParserInlineMode::Comment => None,
            ParserInlineMode::InlineText(text) => {
                if text.is_empty() {
                    None
                } else {
                    Some(Token::Text(text))
                }
            }
            ParserInlineMode::InlineCode { content, .. } => Some(Token::Code(content)),
            ParserInlineMode::RawScope { content, .. } => Some(Token::RawScope(content)),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParserAction {
    EndTokenAndNewlineAndStartText,
    EndTokenAndStartText,
    EndTokenAndStartComment,
    EndTokenAndStartCode(usize),
    EndTokenAndStartRawScope(usize),
    EndTokenAndPushNewScope(usize),
    //
    EndTokenAndPopScope,
    NoAction,
}
impl ParserAction {
    fn should_add_newline_token(&self) -> bool {
        *self == Self::EndTokenAndNewlineAndStartText
    }
    fn should_push_scope(&self) -> Option<usize> {
        match self {
            Self::EndTokenAndPushNewScope(hashes) => Some(*hashes),
            _ => None,
        }
    }
    fn should_pop_scope(&self) -> bool {
        *self == Self::EndTokenAndPopScope
    }
    fn should_transition_to_new_mode(&self) -> Option<ParserInlineMode> {
        use ParserAction::*;
        match *self {
            EndTokenAndNewlineAndStartText
            | EndTokenAndStartText
            | EndTokenAndPushNewScope(_)
            | EndTokenAndPopScope => Some(ParserInlineMode::InlineText("".into())),

            EndTokenAndStartComment => Some(ParserInlineMode::Comment),
            EndTokenAndStartCode(closing_hashes) => Some(ParserInlineMode::InlineCode {
                closing_hashes,
                content: "".into(),
            }),
            EndTokenAndStartRawScope(closing_hashes) => Some(ParserInlineMode::RawScope {
                closing_hashes,
                content: "".into(),
            }),

            NoAction => None,
        }
    }
}
struct ParserState {
    pub scope_stack: Vec<ParserScope>,
    pub inline_mode: ParserInlineMode,
    pub tokens: Vec<Token>,
}
impl ParserState {
    pub fn new() -> Self {
        Self {
            scope_stack: vec![],
            inline_mode: Default::default(),
            tokens: vec![],
        }
    }
    fn parse_simple_token<P>(&mut self, data: &str, stok: SimpleToken<P>) -> Result<(), ParseError>
    where
        P: PosnInCharStream,
    {
        use ParserAction::*;
        use SimpleToken::*;

        let expected_scope_close_hashes = self.scope_stack.last().map(|scope| scope.closing_hashes);

        let action = match &mut self.inline_mode {
            ParserInlineMode::InlineText(text) => match stok {
                // State transitions
                Newline(_) => EndTokenAndNewlineAndStartText,
                Hashes(_, _) => EndTokenAndStartComment,
                CodeOpen(_, n) => EndTokenAndStartCode(n),
                RawScopeOpen(_, n) => EndTokenAndStartRawScope(n),

                // Handle scopes
                ScopeOpen(_, n) => EndTokenAndPushNewScope(n),
                ScopeClose(_, n) => match expected_scope_close_hashes {
                    Some(n_expected) if n == n_expected => EndTokenAndPopScope,
                    Some(_) => Err(ParseError::MismatchingScopeClose(n))?,
                    None => Err(ParseError::ScopeCloseOutsideScope)?,
                },

                // Handle invalid code close
                CodeClose(_, _) => Err(ParseError::CodeCloseInText)?,

                // Handle valid text
                OtherText(_) | Backslash(_) => {
                    text.push_str(stok.stringify(data));
                    NoAction
                }
                Escaped(_, escapee) => {
                    text.push_str(escapee.stringify());
                    NoAction
                }
            },
            ParserInlineMode::InlineCode {
                closing_hashes,
                content,
            } => match stok {
                // Close inline code with a token using the same amount of hashes as the opener
                CodeClose(_, n) if n == *closing_hashes => EndTokenAndStartText,
                // If we hit a newline, error out
                Newline(_) => Err(ParseError::NewlineInCode)?,

                // All other tokens treated as python code
                _ => {
                    content.push_str(stok.stringify(data));
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
                closing_hashes,
                content,
            } => match stok {
                // Close inline code with a token using the same amount of hashes as the opener
                ScopeClose(_, n) if n == *closing_hashes => EndTokenAndStartText,
                // If we hit a newline, pass a consistent \n
                Newline(_) => {
                    content.push_str("\n");
                    NoAction
                }
                // All other tokens taken exactly as from the original text
                _ => {
                    content.push_str(stok.stringify(data));
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
            self.find_next_token_stack().push(Token::Newline);
        }
        if action.should_pop_scope() {
            match self.scope_stack.pop() {
                Some(scope) => self
                    .find_next_token_stack()
                    .push(Token::Scope(scope.tokens)),
                None => panic!(
                    "Executed action {:?} which pops scope with no scopes on the stack",
                    action
                ),
            }
        }
        if let Some(closing_hashes) = action.should_push_scope() {
            assert!(
                !action.should_pop_scope(),
                "A ParserAction shouldn't try to push and pop scopes at the same time"
            );
            self.scope_stack.push(ParserScope {
                closing_hashes,
                tokens: vec![],
            })
        }

        Ok(())
    }

    fn find_next_token_stack<'a>(&'a mut self) -> &'a mut Vec<Token> {
        self.scope_stack
            .last_mut()
            .map_or(&mut self.tokens, |scope| &mut scope.tokens)
    }

    pub fn finalize(mut self) -> Result<Vec<Token>, ParseError> {
        match self.inline_mode {
            ParserInlineMode::Comment => {}
            // If we have text pending, put it into self.tokens
            ParserInlineMode::InlineText(_) => {
                self.execute_action(ParserAction::EndTokenAndStartText)?
            }
            // If we're in code or raw scope mode, something went wrong
            ParserInlineMode::InlineCode { .. } => Err(ParseError::EndedInsideCode)?,
            ParserInlineMode::RawScope { .. } => Err(ParseError::EndedInsideRawScope)?,
        };
        match self.scope_stack.last() {
            Some(_) => Err(ParseError::EndedInsideScope)?,
            None => {}
        };
        Ok(self.tokens)
    }
}
