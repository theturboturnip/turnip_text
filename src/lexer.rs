use lexer_rs::PosnInCharStream;
use lexer_rs::SimpleParseError;
use lexer_rs::StreamCharSpan;
use lexer_rs::{CharStream, Lexer, LexerParseResult};

/// Sequences that can define the start of a [SimpleToken]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LexerPrefixSeq {
    /// `\r`
    CarriageReturn,
    /// `\n`
    LineFeed,
    /// `\r\n`,
    CRLF,
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
    /// `r{`
    RSqgOpen,
}
impl LexerPrefixSeq {
    pub fn try_from_char2(ch: char, ch2: Option<char>) -> Option<(Self, usize)> {
        use LexerPrefixSeq::*;
        match ch {
            '\r' => match ch2 {
                Some('\n') => Some((CRLF, 2)),
                _ => Some((CarriageReturn, 1)),
            },
            '\n' => Some((LineFeed, 1)),
            '\\' => Some((Backslash, 1)),
            '[' => Some((SqrOpen, 1)),
            ']' => Some((SqrClose, 1)),
            '{' => Some((SqgOpen, 1)),
            '}' => Some((SqgClose, 1)),
            '#' => Some((Hash, 1)),
            'r' => match ch2 {
                Some('{') => Some((RSqgOpen, 2)),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn try_extract<L, P>(stream: &L, state: L::State, ch: char) -> Option<(Self, usize)>
    where
        P: PosnInCharStream,
        L: CharStream<P>,
        L: Lexer<Token = SimpleToken<P>, State = P>,
    {
        Self::try_from_char2(ch, stream.peek_at(&stream.consumed(state, 1)))
    }
}

/// Characters/sequences that usually have special meaning in the lexer and can be backslash-escaped
///
/// [LexerPrefixSeq::RSqgOpen] is *not* included, because that is part of a raw scope open 'r{" which is escaped by escaping the '{' not the 'r'.
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
    pub fn stringify(self) -> &'static str {
        match self {
            // An escaped newline = no newline
            Escapable::Newline => "",
            Escapable::Backslash => "\\",
            Escapable::SqrOpen => "[",
            Escapable::SqrClose => "]",
            Escapable::SqgOpen => "{",
            Escapable::SqgClose => "}",
            Escapable::Hash => "#",
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
    /// `[` character not preceded by a backslash plus N # characters
    CodeOpen(StreamCharSpan<P>, usize),
    /// `]` character not preceded by a backslash, preceded by N # characters
    CodeClose(StreamCharSpan<P>, usize),
    /// `{` character not preceded by a backslash plus N # characters
    ScopeOpen(StreamCharSpan<P>, usize),
    /// `r{` sequence plus N # characters
    ///
    /// Escaped by escaping the SqgOpen - `r\{`
    RawScopeOpen(StreamCharSpan<P>, usize),
    /// `}` character not preceded by a backslash, preceded by N # characters
    ScopeClose(StreamCharSpan<P>, usize),
    /// String of N `#` characters not preceded by a backslash
    Hashes(StreamCharSpan<P>, usize),
    /// Span of characters not included in [LexerPrefixSeq]
    OtherText(StreamCharSpan<P>),
    // TODO
    // /// `%` character not preceded by a backslash
    // Percent(P),
}
impl<P> SimpleToken<P>
where
    P: PosnInCharStream,
{
    fn parse_hashes<L>(stream: &L, state: L::State) -> LexerParseResult<P, usize, L::Error>
    where
        L: CharStream<P>,
        L: Lexer<Token = Self, State = P>,
    {
        if let Some(ch) = stream.peek_at(&state) {
            match stream.do_while(state, ch, &|_, ch| ch == '#') {
                (end, Some((_, n))) => Ok(Some((end, n))),
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }
    pub fn parse_special<L>(
        stream: &L,
        state: L::State,
        ch: char,
    ) -> LexerParseResult<P, Self, L::Error>
    where
        L: CharStream<P>,
        L: Lexer<Token = Self, State = P>,
    {
        if let Some((seq, n_chars_in_seq)) = LexerPrefixSeq::try_extract(stream, state, ch) {
            let start = state;
            let state_after_seq = stream.consumed(state, n_chars_in_seq);
            let seq_span = StreamCharSpan::new(start, state_after_seq);
            match seq {
                // Backslash => check if the following character is special, in which case Escaped(), else Backslash()
                LexerPrefixSeq::Backslash => {
                    match Escapable::try_extract(stream, state_after_seq) {
                        Some((escapable, n_chars)) => {
                            // Consume the initial backslash + the number of characters in the escaped sequence
                            let end = stream.consumed(state, n_chars + 1);
                            let span = StreamCharSpan::new(start, end);
                            Ok(Some((end, Self::Escaped(span, escapable))))
                        }
                        None => Ok(Some((state_after_seq, Self::Backslash(state)))),
                    }
                }
                // CRLF or (CR outside CRLF) or (LF outside CRLF) => Newline()
                LexerPrefixSeq::CRLF
                | LexerPrefixSeq::CarriageReturn
                | LexerPrefixSeq::LineFeed => Ok(Some((state_after_seq, Self::Newline(seq_span)))),
                // SqrOpen => CodeOpen(), optionally grab hashes
                LexerPrefixSeq::SqrOpen => {
                    let (end, n) = match Self::parse_hashes(stream, state_after_seq)? {
                        Some((end, n)) => (end, n),
                        None => (state_after_seq, 0),
                    };
                    let span = StreamCharSpan::new(start, end);
                    Ok(Some((end, Self::CodeOpen(span, n))))
                }
                // SqrOpen => ScopeOpen(), optionally grab hashes
                LexerPrefixSeq::SqgOpen => {
                    let (end, n) = match Self::parse_hashes(stream, state_after_seq)? {
                        Some((end, n)) => (end, n),
                        None => (state_after_seq, 0),
                    };
                    let span = StreamCharSpan::new(start, end);
                    Ok(Some((end, Self::ScopeOpen(span, n))))
                }
                // 'r{' => RawScopeOpen(), optionally grab hashes
                LexerPrefixSeq::RSqgOpen => {
                    let (end, n) = match Self::parse_hashes(stream, state_after_seq)? {
                        Some((end, n)) => (end, n),
                        None => (state_after_seq, 0),
                    };
                    let span = StreamCharSpan::new(start, end);
                    Ok(Some((end, Self::RawScopeOpen(span, n))))
                }
                // SqrClose => CodeClose
                LexerPrefixSeq::SqrClose => {
                    Ok(Some((state_after_seq, Self::CodeClose(seq_span, 0))))
                }
                // SqgClose => ScopeClose
                LexerPrefixSeq::SqgClose => {
                    Ok(Some((state_after_seq, Self::ScopeClose(seq_span, 0))))
                }
                // Hash => Hashes or CodeClose or ScopeClose
                LexerPrefixSeq::Hash => {
                    // Run will have at least one hash, because it's starting with this character.
                    match Self::parse_hashes(stream, state)? {
                        // Check the character after the hashes
                        Some((hash_end, n)) => match stream.peek_at(&hash_end) {
                            // String of hashes ending in ']' = CodeClose
                            Some(']') => {
                                let end = stream.consumed(hash_end, 1);
                                let span = StreamCharSpan::new(start, end);
                                Ok(Some((end, Self::CodeClose(span, n))))
                            }
                            // String of hashes ending in '}' = ScopeClose
                            Some('}') => {
                                let end = stream.consumed(hash_end, 1);
                                let span = StreamCharSpan::new(start, end);
                                Ok(Some((end, Self::ScopeClose(span, n))))
                            }
                            _ => Ok(Some((
                                hash_end,
                                Self::Hashes(StreamCharSpan::new(start, hash_end), n),
                            ))),
                        },
                        None => unreachable!(),
                    }
                }
            }
        } else {
            Ok(None)
        }
    }
    pub fn parse_other<L>(
        stream: &L,
        start: L::State,
        start_ch: char,
    ) -> LexerParseResult<P, Self, L::Error>
    where
        L: CharStream<P>,
        L: Lexer<Token = Self, State = P>,
    {
        // This function moves an `end` stream-state forward until it reaches
        // 1) the end of the stream
        // 2) the start of a two-character sequence matched by LexerPrefixSeq
        //
        // This is then used to either construct Some(Other(StreamCharSpan(start, end))), or None if start == end.
        // I *believe* StreamCharSpan(start, end) is [inclusive, exclusive).
        let mut end = start;
        let mut end_ch = start_ch;
        loop {
            // Peek at the next character/stream state
            let next = stream.consumed(end, 1);
            let next_ch = stream.peek_at(&next);

            // See if (end_ch, next_ch) is a prefix sequence
            if LexerPrefixSeq::try_from_char2(end_ch, next_ch).is_some() {
                // if so, break so we can construct a stream for [start_ch, end_ch)
                break;
            }

            // Move forward by one character
            // If next_ch == None, this means 'end' points to the end of the stream.
            // We check for this and break immediately after this, but it's important that 'end' points to the stream end when we break
            // Otherwise we will miss a character off the end
            end = next;
            // See if we've hit the end of the stream
            match next_ch {
                Some(ch2) => end_ch = ch2,
                None => break, // End of stream => break
            }
        }
        if start == end {
            Ok(None)
        } else {
            let span = StreamCharSpan::new(start, end);
            Ok(Some((end, Self::OtherText(span))))
        }
    }

    pub fn stringify<'a>(&self, data: &'a str) -> &'a str {
        use SimpleToken::*;
        match self {
            Escaped(span, _)
            | RawScopeOpen(span, _)
            | CodeOpen(span, _)
            | CodeClose(span, _)
            | ScopeOpen(span, _)
            | ScopeClose(span, _)
            | Hashes(span, _)
            | OtherText(span) => &data[span.byte_range()],
            Backslash(_) => "\\",
            Newline(_) => "\n",
        }
    }
}

pub type LexPosn = lexer_rs::StreamCharPos<lexer_rs::LineColumn>;
pub type LexToken = SimpleToken<LexPosn>;
pub type LexError = SimpleParseError<LexPosn>;
