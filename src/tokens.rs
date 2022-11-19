use lexer_rs::PosnInCharStream;
use lexer_rs::StreamCharSpan;
use lexer_rs::{CharStream, Lexer, LexerParseResult};

/// Single unicode code points that have special meaning in the lexer
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SpecialChar {
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
impl SpecialChar {
    pub fn try_from_char(x: char) -> Option<Self> {
        use SpecialChar::*;
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
    /// Backslash-escaped special character
    ///
    /// This only counts a backslash preceding one of the characters covered by [SpecialChar].
    /// Backslash is also a special character, thus `\\[` is an `Escaped(Backslash)` followed by `CodeOpen`.
    /// A backslash followed by any other character (e.g. `\abc`) is treated as a plain `Backslash` followed by `Other`.
    ///
    /// Current special characters are `[], {}, \, #, \r, \n`
    /// TODO include `%` when [SimpleToken::Percent] is uncommented
    Escaped(StreamCharSpan<P>, SpecialChar),
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
        match SpecialChar::try_from_char(ch) {
            // Backslash => check if the following character is special, in which case Escaped(), else Backslash()
            Some(SpecialChar::Backslash) => {
                match stream
                    .peek_at(&stream.consumed(state, 1))
                    .map(SpecialChar::try_from_char)
                {
                    // TODO backslash-newline may not work correctly on windows
                    // on Unix, backslash-newline would be Escaped(LineFeed) which is... ok?
                    // but on Windows backslash-CRLF would be Escaped(CarriageReturn), Newline - if one wanted to do python-esque newline escapes then this would be wrong
                    // Some(Some(SpecialChar::CarriageReturn)) | Some(Some(SpecialChar::LineFeed)) => {
                    // }
                    Some(Some(special)) => {
                        let end = stream.consumed(state, 2);
                        let span = StreamCharSpan::new(state, end);
                        Ok(Some((end, Self::Escaped(span, special))))
                    }
                    _ => Ok(Some((stream.consumed(state, 1), Self::Backslash(state)))),
                }
            }
            // Carriage return => check if the following character is line feed, if so Newline() of \r\n, else Newline() of \r
            Some(SpecialChar::CarriageReturn) => match stream.peek_at(&stream.consumed(state, 1)) {
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
            },
            // Line feed (not participating in an \r\n) => Newline() of \n
            Some(SpecialChar::LineFeed) => {
                let end = stream.consumed(state, 1);
                let span = StreamCharSpan::new(state, end);
                Ok(Some((end, Self::Newline(span))))
            }
            // SqrOpen => CodeOpen(), optionally consume Hash characters afterward
            Some(SpecialChar::SqrOpen) => {
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
            Some(SpecialChar::SqgOpen) => {
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
            Some(SpecialChar::Hash) => {
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
            Some(SpecialChar::SqrClose) => {
                let end = stream.consumed(state, 1);
                let span = StreamCharSpan::new(state, end);
                Ok(Some((end, Self::CodeClose { pos: span, n: 0 })))
            }
            // SqgClose when not participating in a Hash string => ScopeClose
            Some(SpecialChar::SqgClose) => {
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
        match stream.do_while(state, ch, &|_, ch| SpecialChar::try_from_char(ch).is_none()) {
            (state, Some((start, _n))) => {
                let span = StreamCharSpan::new(start, state);
                Ok(Some((state, Self::Other(span))))
            }
            (_, None) => Ok(None),
        }
    }
}
