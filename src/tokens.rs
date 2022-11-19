use lexer_rs::PosnInCharStream;
use lexer_rs::StreamCharSpan;
use lexer_rs::{CharStream, Lexer, LexerParseResult};

/// Single unicode code points that can define the start of a [SimpleToken]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LexerPrefixChar {
    /// `\r`
    CarriageReturn,
    /// `\n`
    LineFeed,
    /// `\`
    Backslash,
    /// `[`
    SqrOpen,
    /// `]`
    SqrClose,
    /// `{`
    SqgOpen,
    /// `}`
    SqgClose,
    /// `#`
    Hash,
}
impl LexerPrefixChar {
    pub fn try_from_char(x: char) -> Option<Self> {
        use LexerPrefixChar::*;
        match x {
            '\r' => Some(CarriageReturn),
            '\n' => Some(LineFeed),
            '\\' => Some(Backslash),
            '[' => Some(SqrOpen),
            ']' => Some(SqrClose),
            '{' => Some(SqgOpen),
            '}' => Some(SqgClose),
            '#' => Some(Hash),
            _ => None,
        }
    }
}

/// Characters/sequences that usually have special meaning in the lexer and can be backslash-escaped
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Escapable {
    /// `\r`, `\r\n`, `\n`
    Newline,
    /// `\`
    Backslash,
    /// `[`
    SqrOpen,
    /// `]`
    SqrClose,
    /// `{`
    SqgOpen,
    /// `}`
    SqgClose,
    /// `#`
    Hash,
}
impl Escapable {
    pub fn try_extract<L, P>(stream: &L, state_of_escapee: L::State) -> Option<(Self, usize)>
    where
        P: PosnInCharStream,
        L: CharStream<P>,
        L: Lexer<Token = SimpleToken<P>, State = P>,
    {
        use Escapable::*;
        match stream.peek_at(&state_of_escapee)? {
            '\r' => match stream.peek_at(&stream.consumed(state_of_escapee, 1)) {
                Some('\n') => Some((Newline, 2)),
                _ => Some((Newline, 1)),
            },
            '\n' => Some((Newline, 1)),
            '\\' => Some((Backslash, 1)),
            '[' => Some((SqrOpen, 1)),
            ']' => Some((SqrClose, 1)),
            '{' => Some((SqgOpen, 1)),
            '}' => Some((SqgClose, 1)),
            '#' => Some((Hash, 1)),
            _ => None,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum SimpleToken<P>
where
    P: PosnInCharStream,
{
    /// `\r\n`, `\n`, or `\r`, supports all for windows compatability
    ///
    /// Note that these are not the literal sequences e.g. ('\\', 'n') - they are the single characters.
    /// The literal sequence of characters ('\\', 'n') would parse to (Backslash, Other)
    ///
    /// [https://stackoverflow.com/a/44996251/4248422]
    Newline(StreamCharSpan<P>),
    /// Backslash-escaped escapable character
    ///
    /// This only counts a backslash preceding one of the characters covered by [Escapable].
    /// Backslash is also a special character, thus `\\[` is an `Escaped(Backslash)` followed by `CodeOpen`.
    /// A backslash followed by any other character (e.g. `\abc`) is treated as a plain `Backslash` followed by `Other`.
    ///
    /// Current escapable characters/sequences are `[, ], {, }, \, #, \r, \n, \r\n`.
    /// TODO include `%` when [SimpleToken::Percent] is uncommented
    Escaped(StreamCharSpan<P>, Escapable),
    /// '\' that does not participate in a [Self::Escaped]
    ///
    /// TODO - A LaTeX-output backend could choose to disallow plain backslashes, as they would interact with LaTeX in potentially unexpected ways.
    Backslash(P),
    /// `[` character not preceded by a backslash, plus N # characters
    ///
    /// Regex (sans backslash handling): `\[#*`
    CodeOpen { pos: StreamCharSpan<P>, n: usize },
    /// `]` character not preceded by a backslash, plus N # characters
    ///
    /// Regex (sans backslash handling): `#*\]`
    CodeClose { pos: StreamCharSpan<P>, n: usize },
    /// `{` character not preceded by a backslash, plus N # characters
    ///
    /// Regex (sans backslash handling): `\{#*`
    ScopeOpen { pos: StreamCharSpan<P>, n: usize },
    /// `}` character not preceded by a backslash, plus N # characters
    ///
    /// Regex (sans backslash handling): `#*\}`
    ScopeClose { pos: StreamCharSpan<P>, n: usize },
    /// String of N `#` characters not preceded by a backslash that is not a participant in [CodeOpen], [CodeClose], [ScopeOpen], [ScopeClose]
    Hashes(StreamCharSpan<P>, usize),
    /// Span of characters not included in [SpecialChar]
    Other(StreamCharSpan<P>),
    // TODO
    // /// `%` character not preceded by a backslash
    // Percent(P),
}
impl<P> SimpleToken<P>
where
    P: PosnInCharStream,
{
    pub fn parse_special<L>(
        stream: &L,
        state: L::State,
        ch: char,
    ) -> LexerParseResult<P, Self, L::Error>
    where
        L: CharStream<P>,
        L: Lexer<Token = Self, State = P>,
    {
        match LexerPrefixChar::try_from_char(ch) {
            // Backslash => check if the following character is special, in which case Escaped(), else Backslash()
            Some(LexerPrefixChar::Backslash) => {
                let state_of_escapee = stream.consumed(state, 1);
                match Escapable::try_extract(stream, state_of_escapee) {
                    Some((escapable, n_chars)) => {
                        // Consume the initial backslash + the number of characters in the escaped sequence
                        let end = stream.consumed(state, n_chars + 1);
                        let span = StreamCharSpan::new(state, end);
                        Ok(Some((end, Self::Escaped(span, escapable))))
                    }
                    None => Ok(Some((stream.consumed(state, 1), Self::Backslash(state)))),
                }
            }
            // Carriage return => check if the following character is line feed, if so Newline() of \r\n, else Newline() of \r
            Some(LexerPrefixChar::CarriageReturn) => {
                match stream.peek_at(&stream.consumed(state, 1)) {
                    Some('\n') => {
                        let end = stream.consumed(state, 2);
                        let span = StreamCharSpan::new(state, end);
                        Ok(Some((end, Self::Newline(span))))
                    }
                    _ => {
                        let end = stream.consumed(state, 1);
                        let span = StreamCharSpan::new(state, end);
                        Ok(Some((end, Self::Newline(span))))
                    }
                }
            }
            // Line feed (not participating in an \r\n) => Newline() of \n
            Some(LexerPrefixChar::LineFeed) => {
                let end = stream.consumed(state, 1);
                let span = StreamCharSpan::new(state, end);
                Ok(Some((end, Self::Newline(span))))
            }
            // SqrOpen => CodeOpen(), optionally consume Hash characters afterward
            Some(LexerPrefixChar::SqrOpen) => {
                match stream.do_while(state, ch, &|n, ch| n == 0 || ch == '#') {
                    (state, Some((start, n))) => {
                        let span = StreamCharSpan::new(start, state);
                        Ok(Some((
                            state,
                            Self::CodeOpen {
                                pos: span,
                                n: n - 1,
                            },
                        )))
                    }
                    (_, None) => unreachable!(),
                }
            }
            // SqrOpen => ScopeOpen(), optionally consume Hash characters afterward
            Some(LexerPrefixChar::SqgOpen) => {
                match stream.do_while(state, ch, &|n, ch| n == 0 || ch == '#') {
                    (state, Some((start, n))) => {
                        let span = StreamCharSpan::new(start, state);
                        Ok(Some((
                            state,
                            Self::ScopeOpen {
                                pos: span,
                                n: n - 1,
                            },
                        )))
                    }
                    (_, None) => unreachable!(),
                }
            }
            // Hash => CodeClose() if followed by hashes then ], ScopeClose if followed by hashes then }, else Hash
            Some(LexerPrefixChar::Hash) => {
                // Run will have at least one hash, because it's starting with this character.
                let (hash_run_end_pos, hash_run_end_result) =
                    stream.do_while(state, ch, &|_, ch| ch == '#');
                let (_, hash_run_n) = hash_run_end_result.unwrap();

                match stream.peek_at(&hash_run_end_pos) {
                    Some(']') => {
                        // Consume the extra ] character
                        let end = stream.consumed(hash_run_end_pos, 1);
                        let span = StreamCharSpan::new(state, end);
                        Ok(Some((
                            end,
                            Self::CodeClose {
                                pos: span,
                                n: hash_run_n,
                            },
                        )))
                    }
                    Some('}') => {
                        // Consume the extra } character
                        let end = stream.consumed(hash_run_end_pos, 1);
                        let span = StreamCharSpan::new(state, end);
                        Ok(Some((
                            end,
                            Self::ScopeClose {
                                pos: span,
                                n: hash_run_n,
                            },
                        )))
                    }
                    _ => {
                        let span = StreamCharSpan::new(state, hash_run_end_pos);
                        Ok(Some((hash_run_end_pos, Self::Hashes(span, hash_run_n))))
                    }
                }
            }
            // SqrClose when not participating in a Hash string => CodeClose
            Some(LexerPrefixChar::SqrClose) => {
                let end = stream.consumed(state, 1);
                let span = StreamCharSpan::new(state, end);
                Ok(Some((end, Self::CodeClose { pos: span, n: 0 })))
            }
            // SqgClose when not participating in a Hash string => ScopeClose
            Some(LexerPrefixChar::SqgClose) => {
                let end = stream.consumed(state, 1);
                let span = StreamCharSpan::new(state, end);
                Ok(Some((end, Self::ScopeClose { pos: span, n: 0 })))
            }
            None => Ok(None),
        }
    }
    pub fn parse_other<L>(
        stream: &L,
        state: L::State,
        ch: char,
    ) -> LexerParseResult<P, Self, L::Error>
    where
        L: CharStream<P>,
        L: Lexer<Token = Self, State = P>,
    {
        match stream.do_while(state, ch, &|_, ch| {
            LexerPrefixChar::try_from_char(ch).is_none()
        }) {
            (state, Some((start, _n))) => {
                let span = StreamCharSpan::new(start, state);
                Ok(Some((state, Self::Other(span))))
            }
            (_, None) => Ok(None),
        }
    }
}
