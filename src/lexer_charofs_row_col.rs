//a Imports
use lexer_rs::UserPosn;

//a LineColumn
//tp LineColumn
/// A line and column within a text stream
///
/// This provides the [UserPosn] trait, which provides methods to
/// retrieve the line and column values of the state.
/// 
/// It also stores the char offset in the stream, not just the byte offset like lexer_rs::StreamCharPos,
/// because this is sometimes required by e.g. error reporting libraries like annotate_snippet
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct LineColumnChar {
    line: usize,
    column: usize,
    char_ofs: usize,
}

impl LineColumnChar {
    pub fn char_ofs(&self) -> usize {
        self.char_ofs
    }
}

//ip Default for LineColumn
impl std::default::Default for LineColumnChar {
    fn default() -> Self {
        Self { line: 1, column: 1, char_ofs: 0, }
    }
}

//ip Display for LineColumn
impl std::fmt::Display for LineColumnChar {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(fmt, "line {} column {} (char {})", self.line, self.column, self.char_ofs)
    }
}

//ip UserPosn for LineColumn
impl UserPosn for LineColumnChar {
    fn line(&self) -> usize {
        self.line
    }

    fn column(&self) -> usize {
        self.column
    }

    fn advance_cols(mut self, _: usize, num_chars: usize) -> Self {
        self.char_ofs += num_chars;
        self.column += num_chars;
        self
    }
    fn advance_line(mut self, num_bytes: usize) -> Self {
        // We assume advance_line() is only called when finding '\n',
        // which is a single character and a single byte.
        assert_eq!(num_bytes, 1);
        self.char_ofs += 1;
        self.column = 1;
        self.line += 1;
        self
    }
    fn error_fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(fmt, "line {} column {}", self.line, self.column)
    }
}
