use lexer_rs::LexerError;
use lexer_rs::LexerOfStr;
use lexer_rs::PosnInCharStream;
use lexer_rs::{CharStream, Lexer, LexerParseResult};

mod line_col_char_posn;
pub use line_col_char_posn::LineColumnChar;
use thiserror::Error;

use crate::util::ParsePosn;
use crate::util::ParseSpan;

pub enum LexedStrIterator {
    Tokenizing(Box<dyn Iterator<Item = TTToken>>),
    Exhausted,
}
impl Iterator for LexedStrIterator {
    type Item = TTToken;

    fn next(&mut self) -> Option<Self::Item> {
        let (next, ret) = match std::mem::replace(self, LexedStrIterator::Exhausted) {
            LexedStrIterator::Exhausted => (LexedStrIterator::Exhausted, None),
            LexedStrIterator::Tokenizing(mut iter) => match iter.next() {
                Some(tok) => (LexedStrIterator::Tokenizing(iter), Some(tok)),
                None => (LexedStrIterator::Exhausted, None),
            },
        };
        *self = next;
        ret
    }
}
/// Right now lexing holds all the tokens in a vector. This sucks, but it's not really avoidable unless we start screwing around with lifetimes inside iterators that match the String in the same field of a struct... argh! :)
pub fn lex(file_idx: usize, data: &str) -> Result<LexedStrIterator, LexError> {
    let lexer = LexerOfStr::<LexPosn, LexToken, LexError>::new(data);

    let mut toks: Vec<TTToken> = vec![];
    for tok in lexer.iter(&[
        Box::new(|stream, state, ch| TTToken::parse_special(file_idx, stream, state, ch)),
        Box::new(|stream, state, ch| TTToken::parse_other(file_idx, stream, state, ch)),
    ]) {
        toks.push(tok?)
    }

    // Add an EOF unit to the end of the stream, with a zero-length ParseSpan at the end of the final character
    // Codespan error reporting displays this properly.
    let eof_span = match toks.last() {
        Some(last_tok) => {
            let end_of_last_tok = last_tok.token_span().end();
            ParseSpan::new(file_idx, end_of_last_tok, end_of_last_tok)
        }
        None => {
            let zero_posn = ParsePosn { byte_ofs: 0 };
            ParseSpan::new(file_idx, zero_posn, zero_posn)
        }
    };
    toks.push(TTToken::EOF(eof_span));

    Ok(LexedStrIterator::Tokenizing(Box::new(toks.into_iter())))
}

/// Sequences that can define the start of a non-text [TTToken]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LexerPrefixSeq {
    /// `\0`
    /// Not actually a prefix, but it's defined here so it interrupts strings of non-special characters.
    /// Always raises [LexError::NullByteFound]
    NulByte,
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
    /// `-`
    HyphenMinus,
}
impl LexerPrefixSeq {
    pub fn try_from_char2(ch: char, ch2: Option<char>) -> Option<(Self, usize)> {
        use LexerPrefixSeq::*;
        match ch {
            '\0' => Some((NulByte, 1)),
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
            '-' => Some((HyphenMinus, 1)),
            x if x.is_whitespace() => Some((Whitespace, 1)),
            _ => None,
        }
    }

    pub fn try_extract<L, P>(stream: &L, state: L::State, ch: char) -> Option<(Self, usize)>
    where
        P: PosnInCharStream,
        L: CharStream<P>,
        L: Lexer<Token = TTToken, State = P>,
    {
        Self::try_from_char2(ch, stream.peek_at(&stream.consumed(state, 1)))
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
    /// `-`
    HyphenMinus,
}
impl Escapable {
    pub fn try_extract<L, P>(stream: &L, state_of_escapee: L::State) -> Option<(Self, usize)>
    where
        P: PosnInCharStream,
        L: CharStream<P>,
        L: Lexer<Token = TTToken, State = P>,
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
            '-' => Some((HyphenMinus, 1)),
            _ => None,
        }
    }
}

pub type LexPosn = lexer_rs::StreamCharPos<LineColumnChar>;
pub type LexToken = TTToken;

// FUTURE consider soft-hyphen
#[derive(Debug, Copy, Clone)]
pub enum TTToken {
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
    Escaped(ParseSpan, Escapable),
    /// '\' that does not participate in a [Self::Escaped]
    Backslash(ParseSpan),
    /// N `-` characters preceding a `[` character, none preceded by a backslash
    CodeOpen(ParseSpan, usize),
    /// `]` character followed by N `-` characters, none preceded by a backslash
    CodeClose(ParseSpan, usize),
    /// `{` character not preceded by a hash or backslash
    ScopeOpen(ParseSpan),
    /// `}` character not preceded by a hash or backslash
    ScopeClose(ParseSpan),
    /// N `#` characters followed by `{`, none preceded by a backslash
    RawScopeOpen(ParseSpan, usize),
    /// `}` character followed by N `#` characters, none preceded by backslash
    RawScopeClose(ParseSpan, usize),
    /// N `#` characters, none preceded by a backslash
    Hashes(ParseSpan, usize),
    /// 2 ASCII hyphen-minus '--' characters, none preceded by backslash
    ///
    /// When stringified translates to Unicode en-dash \u{2013}, unless raw in which case it returns '--'
    EnDash(ParseSpan),
    /// 3 ASCII hyphen-minus '--' characters, none preceded by backslash
    ///
    /// When stringified translates to Unicode em-dash \u{2014}, unless raw in which case it returns '---'
    EmDash(ParseSpan),
    /// N ASCII hyphen-minus '-' characters, none preceded by backslash,
    /// where N == 1 or N > 3.
    /// i.e. any string of HyphenMinuses that isn't a dash.
    ///
    /// When stringified translates to a string of ASCII/Unicode hyphen-minus
    /// Note that different backends handle this differently: LaTeX treats hyphen-minus as a hyphen, Typst treats it as Unicode minus \u{2212}.
    /// Typst has/has issues with emitting actual hyphen-minus, because some PDF viewers treat that as an actual hyphenation character and omit it when copy-pasting.
    /// https://github.com/typst/typst/issues/1267
    /// It also sounds like the Unicode hyphen \u{2010} doesn't have support in a lot of fonts.
    /// If I did translate this to Unicode-hyphen then in LaTeX and Markdown I'd
    /// just have to convert it back. For now, hyphen-minus is fine.
    HyphenMinuses(ParseSpan, usize),
    /// Span of characters not included in [LexerPrefixSeq]
    OtherText(ParseSpan),
    /// String of non-escaped, whitespace, non-[Self::Newline] characters.
    /// The definition of whitespace comes from [char::is_whitespace], i.e. from Unicode
    Whitespace(ParseSpan),
    /// End-of-file token, with a zero-length ParseSpan at the last byte of the file
    EOF(ParseSpan),
}
impl TTToken {
    /// Parses a run of N >= 1 instances of the target character, or None.
    fn parse_n_chars<L>(
        stream: &L,
        state: L::State,
        target_ch: char,
    ) -> LexerParseResult<LexPosn, usize, LexError>
    where
        L: CharStream<LexPosn>,
        L: Lexer<Token = Self, State = LexPosn, Error = LexError>,
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
    ) -> LexerParseResult<LexPosn, Self, LexError>
    where
        L: CharStream<LexPosn>,
        L: Lexer<Token = Self, State = LexPosn, Error = LexError>,
    {
        if let Some((seq, n_chars_in_seq)) = LexerPrefixSeq::try_extract(stream, state, ch) {
            let start = state;
            let state_after_seq = stream.consumed(state, n_chars_in_seq);
            let seq_span = ParseSpan::from_lex(file_idx, start, state_after_seq);
            match seq {
                LexerPrefixSeq::NulByte => Err(LexError::NullByteFound),
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
                            // Bare backslash can be ambiguous with something you indended to escape, but the only things you can escape are syntax-defining characters and newlines. A mistaken not-escaping backslash will be clearly evident both in the syntax-colored text and in the output.
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
                // SqrOpen + HyphenMinus * N => CodeOpen(N)
                // SqrOpen                   => CodeOpen(0)
                LexerPrefixSeq::SqrOpen => {
                    match Self::parse_n_chars(stream, state_after_seq, '-')? {
                        Some((hash_end, n)) => Ok(Some((
                            hash_end,
                            Self::CodeOpen(ParseSpan::from_lex(file_idx, start, hash_end), n),
                        ))),
                        // No subsequent hashes, it's a normal scope close
                        None => Ok(Some((state_after_seq, Self::CodeOpen(seq_span, 0)))),
                    }
                }
                // SqrClose => CodeClose(0)
                LexerPrefixSeq::SqrClose => {
                    Ok(Some((state_after_seq, Self::CodeClose(seq_span, 0))))
                }
                // SqgOpen => ScopeOpen
                LexerPrefixSeq::SqgOpen => Ok(Some((state_after_seq, Self::ScopeOpen(seq_span)))),
                // SqgClose + Hash * N => RawScopeClose(N)
                // SqgClose            => ScopeClose
                LexerPrefixSeq::SqgClose => {
                    match Self::parse_n_chars(stream, state_after_seq, '#')? {
                        Some((hash_end, n)) => Ok(Some((
                            hash_end,
                            Self::RawScopeClose(ParseSpan::from_lex(file_idx, start, hash_end), n),
                        ))),
                        // No subsequent hashes, it's a normal scope close
                        None => Ok(Some((state_after_seq, Self::ScopeClose(seq_span)))),
                    }
                }
                // Hash * N + SqgOpen => RawScopeOpen(N)
                // Hash * N           => Hashes(N)
                LexerPrefixSeq::Hash => {
                    // Run will have at least one #, because it's starting with this character.
                    match Self::parse_n_chars(stream, state, '#')? {
                        Some((hash_end, n)) => {
                            // if followed by { it's a raw scope open
                            let state_after_char_after_hash = stream.consumed(hash_end, 1);
                            match stream.peek_at(&hash_end) {
                                Some('{') => Ok(Some((
                                    state_after_char_after_hash,
                                    Self::RawScopeOpen(
                                        ParseSpan::from_lex(
                                            file_idx,
                                            start,
                                            state_after_char_after_hash,
                                        ),
                                        n,
                                    ),
                                ))),
                                _ => Ok(Some((
                                    hash_end,
                                    Self::Hashes(ParseSpan::from_lex(file_idx, start, hash_end), n),
                                ))),
                            }
                        }
                        None => unreachable!("We just peeked a hash, there must be at least one"),
                    }
                }
                // HyphenMinus * N + SqrClose => CodeClose(N)
                // HyphenMinus * N            => HyphenMinuses(N)
                LexerPrefixSeq::HyphenMinus => {
                    // Run will have at least one -, because it's starting with this character.
                    match Self::parse_n_chars(stream, state, '-')? {
                        Some((hyphens_end, n)) => {
                            let hyphens_span = ParseSpan::from_lex(file_idx, start, hyphens_end);
                            // if followed by ] it's a code close
                            let state_after_char_after_hyphens = stream.consumed(hyphens_end, 1);
                            match stream.peek_at(&hyphens_end) {
                                Some(']') => Ok(Some((
                                    state_after_char_after_hyphens,
                                    Self::CodeClose(
                                        ParseSpan::from_lex(
                                            file_idx,
                                            start,
                                            state_after_char_after_hyphens,
                                        ),
                                        n,
                                    ),
                                ))),
                                _ => match n {
                                    0 => {
                                        unreachable!("parse_n_chars would return None in this case")
                                    }
                                    2 => Ok(Some((hyphens_end, Self::EnDash(hyphens_span)))),
                                    3 => Ok(Some((hyphens_end, Self::EmDash(hyphens_span)))),
                                    1 | _ => Ok(Some((
                                        hyphens_end,
                                        Self::HyphenMinuses(hyphens_span, n),
                                    ))),
                                },
                            }
                        }
                        None => unreachable!(
                            "We just peeked a hyphen-minus, there must be at least one"
                        ),
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
    ) -> LexerParseResult<LexPosn, Self, LexError>
    where
        L: CharStream<LexPosn>,
        L: Lexer<Token = Self, State = LexPosn, Error = LexError>,
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

    // pub fn span(&self) -> ParseSpan {
    //     match *self {
    //         Unit::Newline(span) => span,
    //         Unit::Escaped(span, _) => span,
    //         Unit::Backslash(span) => span,
    //         Unit::CodeOpen(span, _) => span,
    //         Unit::CodeClose(span, _) => span,
    //         Unit::ScopeOpen(span) => span,
    //         Unit::ScopeClose(span) => span,
    //         Unit::Hashes(span, _) => span,
    //         Unit::OtherText(span) => span,
    //         Unit::Whitespace(span) => span,
    //         Unit::EOF(span) => span,
    //     }
    // }

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
            TTToken::HyphenMinuses(span, _) => span,
            TTToken::EnDash(span) => span,
            TTToken::EmDash(span) => span,
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
            | HyphenMinuses(span, _)
            | EnDash(span)
            | EmDash(span)
            | OtherText(span) => &data[span.byte_range()],
        }
    }
    /// Convert a token to a [str] representation, usable for normal representation
    /// i.e. with escaping - `TTToken::Escaped(_, Escapable::SqrOpen)` is converted to just `'['` without the escaping backslash.
    /// Converts [EnDash] to Unicode endash, [EmDash] to Unicode emdash, and [HyphenMinus] to ASCII hyphen-minus.
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
                Escapable::HyphenMinus => "-",
            },
            EnDash(_) => "\u{2013}",
            EmDash(_) => "\u{2014}",
            RawScopeOpen(span, _)
            | RawScopeClose(span, _)
            | CodeOpen(span, _)
            | CodeClose(span, _)
            | ScopeOpen(span)
            | ScopeClose(span)
            | Hashes(span, _)
            | Whitespace(span)
            | HyphenMinuses(span, _)
            | OtherText(span) => &data[span.byte_range()],
        }
    }
}

/// The lexer must be *mostly* infallible.
/// There can be *mostly* arbitrary content inside raw scopes, and those still get parsed. The lexer just packages them into tokens.
/// The one exception is the null character '\0', which is rejected because it indicates the string is not valid UTF-8 text. '\0' is not renderable nor useful to insert directly.
/// Rejecting it in the lexer also lets the Python code parser unwrap CStrings from tokens without worrying about nul-correctness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum LexError {
    #[error("Found a null byte '\\0' in turnip_text source code, which isn't allowed. This source is probably corrupted, not a text file, or was read with the wrong encoding.")]
    NullByteFound,
}
// The lexer library forces us to implement LexerError - effectively a default failure for when none of the parser functions return true - but the turnip text lexer explicitly captures all non-special characters as normal text.
// The set of handled characters = union(special characters, other characters) = set of all characters
// Therefore every character will be handled, even if it is handled with an Err.
impl<P> LexerError<P> for LexError {
    fn failed_to_parse(_state: P, _ch: char) -> Self {
        unreachable!("The turnip_text lexer cannot fail.")
    }
}
