//! This module provides helper functions and types for mimicking "real" turnip-text data structures (especially those created in Python) in Rust.
//! The general usage pattern is to define the expected result of your test with these types, then for harness code to execute the necessary Rust+Python and to then convert those results to these types before comparing.

use std::ffi::CString;

use crate::error::interp::{BlockModeElem, InlineModeContext};
use crate::error::{interp::InterpError, TurnipTextError};
use crate::error::{stringify_pyerr, UserPythonCompileMode, UserPythonExecError};
use crate::interpreter::ParsingFile;
use regex::Regex;

use crate::util::{ParseContext, ParseSpan};

use pyo3::prelude::*;

mod python;
pub use python::*;

/// A type mimicking [ParserSpan] for test purposes
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestParseSpan<'a>(pub &'a str);
impl<'a> From<(&ParseSpan, &'a Vec<ParsingFile>)> for TestParseSpan<'a> {
    fn from(value: (&ParseSpan, &'a Vec<ParsingFile>)) -> Self {
        Self(unsafe {
            value.1[value.0.file_idx()]
                .contents()
                .get_unchecked(value.0.byte_range())
        })
    }
}

/// A type mimicking [ParserContext] for test purposes
///
/// .0 = first token
/// .1 = intervening tokens
/// .2 = last token
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestParseContext<'a>(pub &'a str, pub &'a str, pub &'a str);
impl<'a> From<(&ParseContext, &'a Vec<ParsingFile>)> for TestParseContext<'a> {
    fn from(value: (&ParseContext, &'a Vec<ParsingFile>)) -> Self {
        let start: TestParseSpan = (&value.0.first_tok(), value.1).into();

        let middle: TestParseSpan =
            if value.0.first_tok().end().byte_ofs <= value.0.last_tok().start().byte_ofs {
                let middle_span = ParseSpan::new(
                    value.0.first_tok().file_idx(),
                    value.0.first_tok().end(),
                    value.0.last_tok().start(),
                );
                (&middle_span, value.1).into()
            } else {
                TestParseSpan("")
            };

        let end: TestParseSpan = (&value.0.last_tok(), value.1).into();
        Self(start.0, middle.0, end.0)
    }
}

/// A type mimicking [TurnipTextError] for test purposes
///
/// Does not derive
#[derive(Debug, Clone)]
pub enum TestTurnipError<'a> {
    Interp(TestInterpError<'a>),
    UserPython(TestUserPythonExecError<'a>),
    InternalPython(Regex),
}
impl<'a> From<TestInterpError<'a>> for TestTurnipError<'a> {
    fn from(value: TestInterpError<'a>) -> Self {
        Self::Interp(value)
    }
}
impl<'a> From<TestUserPythonExecError<'a>> for TestTurnipError<'a> {
    fn from(value: TestUserPythonExecError<'a>) -> Self {
        Self::UserPython(value)
    }
}
impl<'a> TestTurnipError<'a> {
    pub fn matches(&self, py: Python, other: &TurnipTextError) -> bool {
        match (self, other) {
            (Self::Interp(expected), TurnipTextError::Interp(sources, actual)) => {
                let actual_as_test: TestInterpError<'_> = (actual, sources).into();
                *dbg!(expected) == dbg!(actual_as_test)
            }
            (Self::UserPython(l_err), TurnipTextError::UserPython(sources, r_err)) => {
                l_err.matches(py, r_err, sources)
            }
            (Self::InternalPython(l_pyerr), TurnipTextError::InternalPython(r_pyerr)) => {
                l_pyerr.is_match(&r_pyerr)
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestInlineModeContext<'a> {
    Paragraph(TestParseContext<'a>),
    InlineScope { scope_start: TestParseSpan<'a> },
}
impl<'a> From<(&'a InlineModeContext, &'a Vec<ParsingFile>)> for TestInlineModeContext<'a> {
    fn from(value: (&'a InlineModeContext, &'a Vec<ParsingFile>)) -> Self {
        match value.0 {
            InlineModeContext::Paragraph(c) => Self::Paragraph((c, value.1).into()),
            InlineModeContext::InlineScope { scope_start } => Self::InlineScope {
                scope_start: (scope_start, value.1).into(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TestBlockModeElem<'a> {
    HeaderFromCode(TestParseSpan<'a>),
    Para(TestParseContext<'a>),
    BlockScope(TestParseContext<'a>),
    BlockFromCode(TestParseSpan<'a>),
    SourceFromCode(TestParseSpan<'a>),
    AnyToken(TestParseSpan<'a>),
}
impl<'a> From<(&'a BlockModeElem, &'a Vec<ParsingFile>)> for TestBlockModeElem<'a> {
    fn from(value: (&'a BlockModeElem, &'a Vec<ParsingFile>)) -> Self {
        match value.0 {
            BlockModeElem::HeaderFromCode(s) => Self::HeaderFromCode((s, value.1).into()),
            BlockModeElem::Para(c) => Self::Para((c, value.1).into()),
            BlockModeElem::BlockScope(c) => Self::BlockScope((c, value.1).into()),
            BlockModeElem::BlockFromCode(s) => Self::BlockFromCode((s, value.1).into()),
            BlockModeElem::SourceFromCode(s) => Self::SourceFromCode((s, value.1).into()),
            BlockModeElem::AnyToken(s) => Self::AnyToken((s, value.1).into()),
        }
    }
}

/// A type mimicking [InterpError] for test purposes
#[derive(Debug, Clone, PartialEq)]
pub enum TestInterpError<'a> {
    CodeCloseOutsideCode(TestParseSpan<'a>),
    BlockScopeCloseOutsideScope(TestParseSpan<'a>),
    InlineScopeCloseOutsideScope(TestParseSpan<'a>),
    RawScopeCloseOutsideRawScope(TestParseSpan<'a>),
    EndedInsideCode {
        code_start: TestParseSpan<'a>,
        eof_span: TestParseSpan<'a>,
    },
    EndedInsideRawScope {
        raw_scope_start: TestParseSpan<'a>,
        eof_span: TestParseSpan<'a>,
    },
    EndedInsideScope {
        scope_start: TestParseSpan<'a>,
        eof_span: TestParseSpan<'a>,
    },
    BlockScopeOpenedInInlineMode {
        inl_mode: TestInlineModeContext<'a>,
        block_scope_open: TestParseSpan<'a>,
    },
    CodeEmittedBlockInInlineMode {
        inl_mode: TestInlineModeContext<'a>,
        code_span: TestParseSpan<'a>,
    },
    CodeEmittedHeaderInInlineMode {
        inl_mode: TestInlineModeContext<'a>,
        code_span: TestParseSpan<'a>,
    },
    CodeEmittedHeaderInBlockScope {
        block_scope_start: TestParseSpan<'a>,
        code_span: TestParseSpan<'a>, // TODO should include argument to code_span separately
    },
    CodeEmittedSourceInInlineMode {
        inl_mode: TestInlineModeContext<'a>,
        code_span: TestParseSpan<'a>,
    },
    SentenceBreakInInlineScope {
        scope_start: TestParseSpan<'a>,
        sentence_break: TestParseSpan<'a>,
    },
    EscapedNewlineOutsideParagraph {
        newline: TestParseSpan<'a>,
    },
    InsufficientBlockSeparation {
        last_block: TestBlockModeElem<'a>,
        next_block_start: TestBlockModeElem<'a>,
    },
}
impl<'a> From<(&'a Box<InterpError>, &'a Vec<ParsingFile>)> for TestInterpError<'a> {
    fn from(value: (&'a Box<InterpError>, &'a Vec<ParsingFile>)) -> Self {
        match value.0.as_ref() {
            InterpError::CodeCloseOutsideCode(s) => Self::CodeCloseOutsideCode((s, value.1).into()),
            InterpError::BlockScopeCloseOutsideScope(s) => {
                Self::BlockScopeCloseOutsideScope((s, value.1).into())
            }
            InterpError::InlineScopeCloseOutsideScope(s) => {
                Self::InlineScopeCloseOutsideScope((s, value.1).into())
            }
            InterpError::RawScopeCloseOutsideRawScope(s) => {
                Self::RawScopeCloseOutsideRawScope((s, value.1).into())
            }
            InterpError::EndedInsideCode {
                code_start,
                eof_span,
            } => Self::EndedInsideCode {
                code_start: (code_start, value.1).into(),
                eof_span: (eof_span, value.1).into(),
            },
            InterpError::EndedInsideRawScope {
                raw_scope_start,
                eof_span,
            } => Self::EndedInsideRawScope {
                raw_scope_start: (raw_scope_start, value.1).into(),
                eof_span: (eof_span, value.1).into(),
            },
            InterpError::EndedInsideScope {
                scope_start,
                eof_span,
            } => Self::EndedInsideScope {
                scope_start: (scope_start, value.1).into(),
                eof_span: (eof_span, value.1).into(),
            },
            InterpError::BlockScopeOpenedInInlineMode {
                inl_mode,
                block_scope_open,
            } => Self::BlockScopeOpenedInInlineMode {
                inl_mode: (inl_mode, value.1).into(),
                block_scope_open: (block_scope_open, value.1).into(),
            },
            InterpError::CodeEmittedBlockInInlineMode {
                inl_mode,
                block: _,
                code_span,
            } => Self::CodeEmittedBlockInInlineMode {
                inl_mode: (inl_mode, value.1).into(),
                code_span: (code_span, value.1).into(),
            },
            InterpError::CodeEmittedHeaderInInlineMode {
                inl_mode,
                header: _,
                code_span,
            } => Self::CodeEmittedHeaderInInlineMode {
                inl_mode: (inl_mode, value.1).into(),
                code_span: (code_span, value.1).into(),
            },
            InterpError::CodeEmittedHeaderInBlockScope {
                block_scope_start,
                header: _,
                code_span,
            } => Self::CodeEmittedHeaderInBlockScope {
                block_scope_start: (block_scope_start, value.1).into(),
                code_span: (code_span, value.1).into(),
            },
            InterpError::CodeEmittedSourceInInlineMode {
                inl_mode,
                code_span,
            } => Self::CodeEmittedSourceInInlineMode {
                inl_mode: (inl_mode, value.1).into(),
                code_span: (code_span, value.1).into(),
            },
            InterpError::SentenceBreakInInlineScope {
                scope_start,
                sentence_break,
            } => Self::SentenceBreakInInlineScope {
                scope_start: (scope_start, value.1).into(),
                sentence_break: (sentence_break, value.1).into(),
            },

            InterpError::EscapedNewlineOutsideParagraph { newline } => {
                Self::EscapedNewlineOutsideParagraph {
                    newline: (newline, value.1).into(),
                }
            }
            InterpError::InsufficientBlockSeparation {
                last_block,
                next_block_start,
            } => Self::InsufficientBlockSeparation {
                last_block: (last_block, value.1).into(),
                next_block_start: (next_block_start, value.1).into(),
            },
        }
    }
}

/// The contexts in which you might execute Python on user-generated code or objects
#[derive(Debug, Clone)]
pub enum TestUserPythonExecError<'a> {
    CompilingEvalBrackets {
        code_ctx: TestParseContext<'a>,
        code: CString,
        mode: UserPythonCompileMode,
        err: Regex,
    },
    RunningEvalBrackets {
        code_ctx: TestParseContext<'a>,
        code: CString,
        mode: UserPythonCompileMode,
        err: Regex,
    },
    CoercingNonBuilderEvalBracket {
        code_ctx: TestParseContext<'a>,
    },
    CoercingBlockScopeBuilder {
        code_ctx: TestParseContext<'a>,
        err: Regex,
    },
    CoercingInlineScopeBuilder {
        code_ctx: TestParseContext<'a>,
        err: Regex,
    },
    CoercingRawScopeBuilder {
        code_ctx: TestParseContext<'a>,
        err: Regex,
    },
}
impl<'a> TestUserPythonExecError<'a> {
    pub fn matches(
        &self,
        py: Python,
        other: &'a UserPythonExecError,
        data: &'a Vec<ParsingFile>,
    ) -> bool {
        match (self, other) {
            (
                TestUserPythonExecError::CompilingEvalBrackets {
                    code_ctx: l_code_ctx,
                    code: l_code,
                    mode: l_mode,
                    err: l_err,
                },
                UserPythonExecError::CompilingEvalBrackets {
                    code_ctx: r_code_ctx,
                    code: r_code,
                    mode: r_mode,
                    err: r_err,
                },
            )
            | (
                TestUserPythonExecError::RunningEvalBrackets {
                    code_ctx: l_code_ctx,
                    code: l_code,
                    mode: l_mode,
                    err: l_err,
                },
                UserPythonExecError::RunningEvalBrackets {
                    code_ctx: r_code_ctx,
                    code: r_code,
                    mode: r_mode,
                    err: r_err,
                },
            ) => {
                (*dbg!(l_code_ctx) == dbg!((r_code_ctx, data).into()))
                    && (dbg!(l_code) == dbg!(r_code))
                    && (dbg!(l_mode) == dbg!(r_mode))
                    && dbg!(l_err).is_match(&dbg!(stringify_pyerr(py, r_err)))
            }
            (
                TestUserPythonExecError::CoercingBlockScopeBuilder {
                    code_ctx: l_code,
                    err: l_err,
                },
                UserPythonExecError::CoercingBlockScopeBuilder {
                    code_ctx: r_code,
                    err: r_err,
                    obj: _,
                },
            )
            | (
                TestUserPythonExecError::CoercingInlineScopeBuilder {
                    code_ctx: l_code,
                    err: l_err,
                },
                UserPythonExecError::CoercingInlineScopeBuilder {
                    code_ctx: r_code,
                    err: r_err,
                    obj: _,
                },
            )
            | (
                TestUserPythonExecError::CoercingRawScopeBuilder {
                    code_ctx: l_code,
                    err: l_err,
                },
                UserPythonExecError::CoercingRawScopeBuilder {
                    code: r_code,
                    err: r_err,
                    obj: _,
                },
            ) => {
                (*dbg!(l_code) == dbg!((r_code, data).into()))
                    && dbg!(l_err).is_match(&dbg!(stringify_pyerr(py, r_err)))
            }

            (
                TestUserPythonExecError::CoercingNonBuilderEvalBracket { code_ctx: l_code },
                UserPythonExecError::CoercingNonBuilderEvalBracket {
                    code_ctx: r_code,
                    obj: _,
                },
            ) => *dbg!(l_code) == dbg!((r_code, data).into()),
            _ => false,
        }
    }
}
