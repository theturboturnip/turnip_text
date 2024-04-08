use lexer_rs::LexerOfStr;
use lexer_rs::PosnInCharStream;
use lexer_rs::SimpleParseError;
use lexer_rs::{CharStream, Lexer, LexerParseResult};

mod line_col_char_posn;
use line_col_char_posn::LineColumnChar;

use crate::util::ParsePosn;
use crate::util::ParseSpan;

#[derive(Debug)]
pub enum LexedStrIterator {
    Exhausted,
    Error(LexError),
    Tokenizing(UnitsToTokensIterator),
}
impl Iterator for LexedStrIterator {
    type Item = Result<TTToken, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        let (should_exhaust, ret) = match self {
            LexedStrIterator::Exhausted => (false, None),
            // TODO sucks we have to clone this :/
            LexedStrIterator::Error(e) => (true, Some(Err(e.clone()))),
            LexedStrIterator::Tokenizing(tokenizer) => match tokenizer.next() {
                Some(tok) => (false, Some(Ok(tok))),
                None => (true, None),
            },
        };
        if should_exhaust {
            *self = LexedStrIterator::Exhausted;
        }
        ret
    }
}
/// TODO right now we have to store all the first-level lex tokens in a vector. This sucks.
fn lex_units_only(file_idx: usize, data: &str) -> Result<Vec<Unit>, LexError> {
    let lexer = LexerOfStr::<LexPosn, LexToken, LexError>::new(data);

    let mut units: Vec<Unit> = vec![];
    for u in lexer.iter(&[
        Box::new(|stream, state, ch| Unit::parse_special(file_idx, stream, state, ch)),
        Box::new(|stream, state, ch| Unit::parse_other(file_idx, stream, state, ch)),
    ]) {
        units.push(u?);
    }

    // Add an EOF unit to the end of the stream, with a zero-length ParseSpan at the end of the final character
    let eof_span = match units.last() {
        Some(last_unit) => {
            let end_of_last_unit = last_unit.span().end();
            ParseSpan::new(file_idx, end_of_last_unit, end_of_last_unit)
        }
        None => {
            let zero_posn = ParsePosn {
                byte_ofs: 0,
                char_ofs: 0,
                line: 0,
                column: 0,
            };
            ParseSpan::new(file_idx, zero_posn, zero_posn)
        }
    };
    units.push(Unit::EOF(eof_span));

    Ok(units)
}
pub fn lex(file_idx: usize, data: &str) -> LexedStrIterator {
    let units = lex_units_only(file_idx, data);
    match units {
        Ok(units) => LexedStrIterator::Tokenizing(UnitsToTokensIterator::new(units)),
        Err(e) => LexedStrIterator::Error(e),
    }
}

/// Sequences that can define the start of a [SimpleToken]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LexerPrefixSeq {
    /// `\r`
    CarriageReturn,
    /// `\n`
    LineFeed,
    /// `\r\n`,
    CRLF,
    /// Characters that are not `\r` or `\n` but are otherwise whitespace
    Whitespace,
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
            x if x.is_whitespace() => Some((Whitespace, 1)),
            _ => None,
        }
    }

    pub fn try_extract<L, P>(stream: &L, state: L::State, ch: char) -> Option<(Self, usize)>
    where
        P: PosnInCharStream,
        L: CharStream<P>,
        L: Lexer<Token = Unit, State = P>,
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
    /// Some non-[Self::Newline] whitespace character. When escaped, always resolves to " ".
    Whitespace(char),
}
impl Escapable {
    pub fn try_extract<L, P>(stream: &L, state_of_escapee: L::State) -> Option<(Self, usize)>
    where
        P: PosnInCharStream,
        L: CharStream<P>,
        L: Lexer<Token = Unit, State = P>,
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
            x if x.is_whitespace() => Some((Whitespace(x), 1)),
            _ => None,
        }
    }
}

pub type LexPosn = lexer_rs::StreamCharPos<LineColumnChar>;
pub type LexToken = Unit;
pub type LexError = SimpleParseError<LexPosn>;

#[derive(Debug, Copy, Clone)]
pub enum Unit {
    /// `\r\n`, `\n`, or `\r`, supports all for windows compatability
    ///
    /// Note that these are not the literal sequences e.g. ('\\', 'n') - they are the single characters.
    /// The literal sequence of characters ('\\', 'n') would parse to (Backslash, Other)
    ///
    /// [https://stackoverflow.com/a/44996251/4248422]
    Newline(ParseSpan),
    /// Backslash-escaped escapable character
    ///
    /// This only counts a backslash preceding one of the characters covered by [Escapable].
    /// Backslash is also a special character, thus `\\[` is an `Escaped(Backslash)` followed by `CodeOpen`.
    /// A backslash followed by any other character (e.g. `\abc`) is treated as a plain `Backslash` followed by `Other`.
    ///
    /// Current escapable characters/sequences are `[, ], {, }, \, #, \r, \n, \r\n`.
    /// TODO include `%` when [SimpleToken::Percent] is uncommented
    Escaped(ParseSpan, Escapable),
    /// '\' that does not participate in a [Self::Escaped]
    ///
    /// TODO - A LaTeX-output backend could choose to disallow plain backslashes, as they would interact with LaTeX in potentially unexpected ways.
    Backslash(ParseSpan),
    /// N `[` characters not preceded by a backslash
    CodeOpen(ParseSpan, usize),
    /// N `]` characters not preceded by a backslash
    CodeClose(ParseSpan, usize),
    /// `{` character not preceded by a backslash
    ScopeOpen(ParseSpan),
    /// `}` character not preceded by a backslash
    ScopeClose(ParseSpan),
    /// N `#` characters not preceded by a backslash
    Hashes(ParseSpan, usize),
    /// Span of characters not included in [LexerPrefixSeq]
    OtherText(ParseSpan),
    /// String of non-escaped, whitespace, non-[Self::Newline] characters.
    /// The definition of whitespace comes from [char::is_whitespace], i.e. from Unicode
    Whitespace(ParseSpan),
    // TODO
    // /// `%` character not preceded by a backslash
    // Percent(P),
    EOF(ParseSpan),
}
impl Unit {
    fn parse_n_chars<L>(
        file_idx: usize,
        stream: &L,
        state: L::State,
        target_ch: char,
    ) -> LexerParseResult<LexPosn, usize, L::Error>
    where
        L: CharStream<LexPosn>,
        L: Lexer<Token = Self, State = LexPosn>,
    {
        if let Some(ch) = stream.peek_at(&state) {
            match stream.do_while(state, ch, &|_, ch| ch == target_ch) {
                (end, Some((_, n))) => Ok(Some((end, n))),
                _ => Ok(None),
            }
        } else {
            Ok(None)
        }
    }
    pub fn parse_special<L>(
        file_idx: usize,
        stream: &L,
        state: L::State,
        ch: char,
    ) -> LexerParseResult<LexPosn, Self, L::Error>
    where
        L: CharStream<LexPosn>,
        L: Lexer<Token = Self, State = LexPosn>,
    {
        if let Some((seq, n_chars_in_seq)) = LexerPrefixSeq::try_extract(stream, state, ch) {
            let start = state;
            let state_after_seq = stream.consumed(state, n_chars_in_seq);
            let seq_span = ParseSpan::from_lex(file_idx, start, state_after_seq);
            match seq {
                // Backslash => check if the following character is special, in which case Escaped(), else Backslash()
                LexerPrefixSeq::Backslash => {
                    match Escapable::try_extract(stream, state_after_seq) {
                        Some((escapable, n_chars)) => {
                            // Consume the initial backslash + the number of characters in the escaped sequence
                            let end = stream.consumed(state, n_chars + 1);
                            let span = ParseSpan::from_lex(file_idx, start, end);
                            Ok(Some((end, Self::Escaped(span, escapable))))
                        }
                        None => {
                            let end = stream.consumed(state, 1);
                            Ok(Some((
                                state_after_seq,
                                Self::Backslash(ParseSpan::from_lex(file_idx, start, end)),
                            )))
                        }
                    }
                }
                // CRLF or (CR outside CRLF) or (LF outside CRLF) => Newline()
                LexerPrefixSeq::CRLF
                | LexerPrefixSeq::CarriageReturn
                | LexerPrefixSeq::LineFeed => Ok(Some((state_after_seq, Self::Newline(seq_span)))),
                // SqrOpen => CodeOpen()
                LexerPrefixSeq::SqrOpen => {
                    // Run will have at least one [, because it's starting with this character.
                    match Self::parse_n_chars(file_idx, stream, state, '[')? {
                        Some((hash_end, n)) => Ok(Some((
                            hash_end,
                            Self::CodeOpen(ParseSpan::from_lex(file_idx, start, hash_end), n),
                        ))),
                        None => unreachable!(),
                    }
                }
                // SqrClose => CodeClose
                LexerPrefixSeq::SqrClose => {
                    // Run will have at least one hash, because it's starting with this character.
                    match Self::parse_n_chars(file_idx, stream, state, ']')? {
                        Some((hash_end, n)) => Ok(Some((
                            hash_end,
                            Self::CodeClose(ParseSpan::from_lex(file_idx, start, hash_end), n),
                        ))),
                        None => unreachable!(),
                    }
                }
                // SqrOpen => ScopeOpen()
                LexerPrefixSeq::SqgOpen => Ok(Some((state_after_seq, Self::ScopeOpen(seq_span)))),
                // SqgClose => ScopeClose
                LexerPrefixSeq::SqgClose => Ok(Some((state_after_seq, Self::ScopeClose(seq_span)))),
                // Hash => Hash
                LexerPrefixSeq::Hash => {
                    // Run will have at least one #, because it's starting with this character.
                    match Self::parse_n_chars(file_idx, stream, state, '#')? {
                        Some((hash_end, n)) => Ok(Some((
                            hash_end,
                            Self::Hashes(ParseSpan::from_lex(file_idx, start, hash_end), n),
                        ))),
                        None => unreachable!(),
                    }
                }
                // Whitespace => Whitespace
                LexerPrefixSeq::Whitespace => {
                    // Match all whitespace except newlines
                    match stream.do_while(state, ch, &|_, ch| {
                        ch.is_whitespace() && ch != '\n' && ch != '\r'
                    }) {
                        // We peeked a Whitespace prefix, so there must be at least one whitespace char
                        (end, Some(_)) => Ok(Some((
                            end,
                            Self::Whitespace(ParseSpan::from_lex(file_idx, start, end)),
                        ))),
                        _ => unreachable!(),
                    }
                }
            }
        } else {
            Ok(None)
        }
    }
    pub fn parse_other<L>(
        file_idx: usize,
        stream: &L,
        start: L::State,
        start_ch: char,
    ) -> LexerParseResult<LexPosn, Self, L::Error>
    where
        L: CharStream<LexPosn>,
        L: Lexer<Token = Self, State = LexPosn>,
    {
        // This function moves an `end` stream-state forward until it reaches
        // 1) the end of the stream
        // 2) the start of a two-character sequence matched by LexerPrefixSeq
        //
        // This is then used to either construct Some(Other(ParseSpan(start, end))), or None if start == end.
        // I *believe* ParseSpan(start, end) is [inclusive, exclusive).
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
            let span = ParseSpan::from_lex(file_idx, start, end);
            Ok(Some((end, Self::OtherText(span))))
        }
    }

    pub fn span(&self) -> ParseSpan {
        match *self {
            Unit::Newline(span) => span,
            Unit::Escaped(span, _) => span,
            Unit::Backslash(span) => span,
            Unit::CodeOpen(span, _) => span,
            Unit::CodeClose(span, _) => span,
            Unit::ScopeOpen(span) => span,
            Unit::ScopeClose(span) => span,
            Unit::Hashes(span, _) => span,
            Unit::OtherText(span) => span,
            Unit::Whitespace(span) => span,
            Unit::EOF(span) => span,
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum TTToken {
    /// See [Unit::Newline]
    Newline(ParseSpan),
    /// See [Unit::Escaped]
    Escaped(ParseSpan, Escapable),
    /// See [Unit::Backslash]
    Backslash(ParseSpan),
    /// N `[` characters not preceded by a backslash
    CodeOpen(ParseSpan, usize),
    /// N `]` characters not preceded by a backslash, not followed by a scope open
    CodeClose(ParseSpan, usize),
    /// `{` character not preceded by a backslash or code
    ScopeOpen(ParseSpan),
    /// `}` character not preceded by a backslash
    ScopeClose(ParseSpan),
    /// N hashes followed by `{` not preceded by a backslash
    RawScopeOpen(ParseSpan, usize),
    /// `}` character followed by N hashes not preceded by backslash
    RawScopeClose(ParseSpan, usize),
    /// See [Unit::Hashes]
    Hashes(ParseSpan, usize),
    /// See [Unit::OtherText]
    OtherText(ParseSpan),
    /// See [Unit::Whitespace]
    Whitespace(ParseSpan),
    // TODO
    // /// `%` character not preceded by a backslash
    // Percent(P),
    EOF(ParseSpan),
}
pub fn units_to_tokens(units: Vec<Unit>) -> Vec<TTToken> {
    let mut toks = vec![];
    let mut i = 0;
    while i < units.len() {
        let (tok, n_consumed) = TTToken::units_to_token((
            &units[i],
            if i + 1 < units.len() {
                Some(&units[i + 1])
            } else {
                None
            },
            if i + 2 < units.len() {
                Some(&units[i + 2])
            } else {
                None
            },
        ));
        assert!(n_consumed > 0);
        toks.push(tok);
        i += n_consumed;
    }
    toks
}

#[derive(Debug)]
pub struct UnitsToTokensIterator {
    units: Vec<Unit>,
    idx: usize,
}
impl UnitsToTokensIterator {
    pub fn new(units: Vec<Unit>) -> UnitsToTokensIterator {
        UnitsToTokensIterator { units, idx: 0 }
    }
}
impl Iterator for UnitsToTokensIterator {
    type Item = TTToken;

    fn next(&mut self) -> Option<Self::Item> {
        let i = self.idx;
        let len = self.units.len();
        if i >= len {
            None
        } else {
            let (tok, n_consumed) = TTToken::units_to_token((
                &self.units[i],
                if i + 1 < self.units.len() {
                    Some(&self.units[i + 1])
                } else {
                    None
                },
                if i + 2 < self.units.len() {
                    Some(&self.units[i + 2])
                } else {
                    None
                },
            ));
            assert!(n_consumed > 0);
            self.idx += n_consumed;
            Some(tok)
        }
    }
}

impl TTToken {
    fn units_to_token(units: (&Unit, Option<&Unit>, Option<&Unit>)) -> (Self, usize) {
        match units {
            (Unit::Newline(s), _, _) => (TTToken::Newline(*s), 1),
            (Unit::Escaped(s, e), _, _) => (TTToken::Escaped(*s, *e), 1),
            (Unit::Backslash(p), _, _) => (TTToken::Backslash(*p), 1),
            (Unit::OtherText(s), _, _) => (TTToken::OtherText(*s), 1),
            (Unit::Whitespace(s), _, _) => (TTToken::Whitespace(*s), 1),

            // Code open and close
            (Unit::CodeOpen(s, n), _, _) => (TTToken::CodeOpen(*s, *n), 1),
            (Unit::CodeClose(span, n), _, _) => (TTToken::CodeClose(*span, *n), 1),

            // Scope open
            (Unit::ScopeOpen(s), _, _) => (TTToken::ScopeOpen(*s), 1),

            // Raw scope open
            (Unit::Hashes(s_start, n), Some(Unit::ScopeOpen(s_end)), _) => {
                (TTToken::RawScopeOpen(s_start.combine(s_end), *n), 2)
            }

            // Scope close
            (Unit::ScopeClose(s_start), Some(Unit::Hashes(s_end, n)), _) => {
                (TTToken::RawScopeClose(s_start.combine(s_end), *n), 2)
            }
            (Unit::ScopeClose(s), _, _) => (TTToken::ScopeClose(*s), 1),

            // Raw scope close
            (Unit::Hashes(s, n), _, _) => (TTToken::Hashes(*s, *n), 1),

            // EOF handling
            (Unit::EOF(s), None, None) => (TTToken::EOF(*s), 1),
            (Unit::EOF(_), _, _) => unreachable!("EOF cannot be followed by any other units"),
        }
    }

    pub fn token_span(&self) -> ParseSpan {
        match *self {
            TTToken::Newline(span) => span,
            TTToken::Escaped(span, _) => span,
            TTToken::Backslash(span) => span,
            TTToken::CodeOpen(span, _) => span,
            TTToken::CodeClose(span, _) => span,
            TTToken::ScopeOpen(span) => span,
            TTToken::ScopeClose(span) => span,
            TTToken::RawScopeOpen(span, _) => span,
            TTToken::RawScopeClose(span, _) => span,
            TTToken::Hashes(span, _) => span,
            TTToken::OtherText(span) => span,
            TTToken::Whitespace(span) => span,
            TTToken::EOF(span) => span,
        }
    }

    /// Convert a token to a [str] representation, usable for a raw-scope representation
    /// i.e. with no escaping - `TTToken::Escaped(_, Escapable::SqrOpen)` is converted to `'\['` with the escaping backslash.
    ///
    /// Newlines are converted to \n everywhere.
    /// EOFs are converted to "".
    pub fn stringify_raw<'a>(&self, data: &'a str) -> &'a str {
        use TTToken::*;
        match self {
            Backslash(_) => "\\",
            Newline(_) => "\n",
            EOF(_) => "",
            // Escaped(Newline) = Backslash() + Newline(), which is always \n
            Escaped(_, Escapable::Newline) => "\\\n",
            Escaped(span, _)
            | RawScopeOpen(span, _)
            | RawScopeClose(span, _)
            | CodeOpen(span, _)
            | CodeClose(span, _)
            | ScopeOpen(span)
            | ScopeClose(span)
            | Hashes(span, _)
            | Whitespace(span)
            | OtherText(span) => &data[span.byte_range()],
        }
    }
    /// Convert a token to a [str] representation, usable for normal representation
    /// i.e. with escaping - `TTToken::Escaped(_, Escapable::SqrOpen)` is converted to just `'['` without the escaping backslash.
    ///
    /// Panics on newlines and escaped newlines as they should always have semantic meaning.
    /// EOFs are converted to "".
    pub fn stringify_escaped<'a>(&self, data: &'a str) -> &'a str {
        use TTToken::*;
        match self {
            Backslash(_) => "\\",
            EOF(_) => "",
            // This is an odd case - Newline should have semantic meaning and not be embedded in text
            Newline(_) => panic!("Newline should not be stringified"),
            Escaped(_, escaped) => match escaped {
                // This is an odd case - Escaped(Newline) should have semantic meaning
                Escapable::Newline => {
                    panic!("EscapedNewline should have semantic meaning and not be stringified")
                }
                Escapable::Backslash => "\\",
                Escapable::SqrOpen => "[",
                Escapable::SqrClose => "]",
                Escapable::SqgOpen => "{",
                Escapable::SqgClose => "}",
                Escapable::Hash => "#",
                Escapable::Whitespace(_) => " ",
            },
            RawScopeOpen(span, _)
            | RawScopeClose(span, _)
            | CodeOpen(span, _)
            | CodeClose(span, _)
            | ScopeOpen(span)
            | ScopeClose(span)
            | Hashes(span, _)
            | Whitespace(span)
            | OtherText(span) => &data[span.byte_range()],
        }
    }
}
