use annotate_snippets::{display_list::DisplayList, snippet::*};
use pyo3::{Python, PyErr};
use thiserror::Error;

use crate::{
    lexer::LexError,
    util::ParseSpan,
    interpreter::InterpError, parser::ParsingFile
};

pub fn stringify_pyerr(py: Python, pyerr: &PyErr) -> String {
    let value = pyerr.value(py);
    let type_name = match value.get_type().name() {
        Ok(name) => name,
        Err(_) => "Unknown Type",
    };
    if let Ok(s) = value.str() {
        format!("{0} : {1}", type_name, &s.to_string_lossy())
    } else {
        "<exception str() failed>".into()
    }
}

#[derive(Error, Debug)]
pub enum TurnipTextContextlessError {
    #[error("Syntax Error: {1}")]
    Lex(usize, LexError),
    #[error("Interpreter Error: {0}")]
    Interp(#[from] Box<InterpError>),
    #[error("Internal Error: {0}")]
    Internal(String),
    #[error("Internal Python Error: {0}")]
    InternalPython(String),
}
impl From<InterpError> for TurnipTextContextlessError {
    fn from(value: InterpError) -> Self {
        Self::Interp(Box::new(value))
    }
}
impl From<(usize, LexError)> for TurnipTextContextlessError {
    fn from(value: (usize, LexError)) -> Self {
        Self::Lex(value.0, value.1)
    }
}
impl From<(Python<'_>, PyErr)> for TurnipTextContextlessError {
    fn from(value: (Python, PyErr)) -> Self {
        Self::InternalPython(stringify_pyerr(value.0, &value.1))
    }
}

pub type TurnipTextContextlessResult<T> = Result<T, TurnipTextContextlessError>;

#[derive(Error, Debug)]
pub enum TurnipTextError {
    #[error("Syntax Error {2}")]
    Lex(Vec<ParsingFile>, usize, LexError),
    #[error("Interpreter Error {1}")]
    Interp(Vec<ParsingFile>, Box<InterpError>),
    #[error("Internal Error {0}")]
    Internal(String),
    #[error("Internal Python Error {0}")]
    InternalPython(String),
}
impl From<(Vec<ParsingFile>, TurnipTextContextlessError)> for TurnipTextError {
    fn from(value: (Vec<ParsingFile>, TurnipTextContextlessError)) -> Self {
        match value.1 {
            TurnipTextContextlessError::Lex(file_idx, err) => Self::Lex(value.0, file_idx, err),
            TurnipTextContextlessError::Interp(err) => Self::Interp(value.0, err),
            TurnipTextContextlessError::Internal(err) => Self::Internal(err),
            TurnipTextContextlessError::InternalPython(err) => Self::InternalPython(err),
        }
    }
}
impl TurnipTextError {
    pub fn display_cli_feedback(&self) {
        let dl = DisplayList::from(self.snippet());
        eprintln!("{}", dl);
    }
    
    fn snippet<'a>(&'a self) -> Snippet<'a> {
        match self {
            TurnipTextError::Lex(sources, file_idx, err) => Self::lex_error_snippet(sources, *file_idx, err),
            TurnipTextError::Interp(sources, err) => Self::interp_error_snippet(sources, err),
            TurnipTextError::Internal(pyerr) => Snippet {
                title: Some(Annotation {
                    label: Some("Internal Python error"),
                    id: None,
                    annotation_type: AnnotationType::Error
                }),
                footer: vec![Annotation{
                    label: Some(&pyerr),
                    id: None,
                    annotation_type: AnnotationType::Error
                }],
                slices: vec![],
                opt: Default::default(),
            },
            TurnipTextError::InternalPython(err) => Snippet {
                title: Some(Annotation {
                    label: Some("Internal error"),
                    id: None,
                    annotation_type: AnnotationType::Error
                }),
                footer: vec![Annotation{
                    label: Some(&err),
                    id: None,
                    annotation_type: AnnotationType::Error
                }],
                slices: vec![],
                opt: Default::default(),
            },
        }
    }
}

pub type TurnipTextResult<T> = Result<T, TurnipTextError>;

fn snippet_from_spans<'a>(
    top_label: &'a str,
    annotation_type: AnnotationType,
    sources: &'a Vec<ParsingFile>,
    spans: &[(&'a ParseSpan, &'a str, Option<AnnotationType>)],
) -> Snippet<'a> {
    Snippet {
        title: Some(Annotation {
            label: Some(top_label),
            id: None,
            annotation_type,
        }),
        footer: vec![],
        slices: slices_from_spans(annotation_type, sources, spans),
        opt: Default::default(),
    }
}

fn slices_from_spans<'a>(
    default_annotation_type: AnnotationType,
    sources: &'a Vec<ParsingFile>,
    spans: &[(&ParseSpan, &'a str, Option<AnnotationType>)],
) -> Vec<Slice<'a>> {
    spans.iter().map(|(span, label, annotation_type)| {
        let file_idx = &sources[span.file_idx()];
        Slice {
            origin: Some(file_idx.name()),
            source: file_idx.contents(),
            line_start: 1,
            fold: true,
            annotations: vec![annotation_from_parse_span(
                *label,
                annotation_type.unwrap_or(default_annotation_type),
                &span,
            )],
        }
    }).collect()
}

fn annotation_from_parse_span<'a>(
    label: &'a str,
    annotation_type: AnnotationType,
    span: &ParseSpan,
) -> SourceAnnotation<'a> {
    SourceAnnotation {
        range: span.annotate_snippets_range(),
        label,
        annotation_type,
    }
}

impl TurnipTextError {
    fn lex_error_snippet<'a>(sources: &'a Vec<ParsingFile>, file_idx: usize, err: &'a LexError) -> Snippet<'a> {
        // TODO - in the event this breaks on non-ASCII/non-single-byte,
        // it would be nice to print the character somewhere
        Snippet {
            title: Some(Annotation {
                label: Some("Parser error"),
                id: None,
                annotation_type: AnnotationType::Error,
            }),
            footer: vec![],
            slices: slices_from_spans(AnnotationType::Error, sources, &[
                (&ParseSpan::single_char(file_idx, err.pos, err.ch), "Unexpected character", None),
            ]),
            opt: Default::default(),
        }
    }

    fn interp_error_snippet<'a>(sources: &'a Vec<ParsingFile>, err: &'a Box<InterpError>) -> Snippet<'a> {
        use InterpError::*;
        match err.as_ref() {
            CodeCloseOutsideCode(span) => snippet_from_spans(

                "Code close token in text mode",
                AnnotationType::Error,
                sources,
&[(span, "", None)]
            ),
            ScopeCloseOutsideScope(span) => snippet_from_spans(
                "Scope close token when outside scope",
                AnnotationType::Error,
                sources,
                &[(span, "", None)],
            ),
            RawScopeCloseOutsideRawScope(span) => snippet_from_spans(

                "Raw scope close token when outside scope",
                AnnotationType::Error,
sources,
&[(span, "", None)]
            ),
            EndedInsideCode { code_start } => snippet_from_spans(

                "File ended inside code block",
                AnnotationType::Error,
sources,
&[(code_start, "Code block starts here", None)]
            ),
            EndedInsideRawScope { raw_scope_start } => snippet_from_spans(

                "File ended inside raw scope",
                AnnotationType::Error,
sources,
&[(raw_scope_start, "Raw scope starts here", None)]
            ),
            EndedInsideScope { scope_start } => snippet_from_spans(

                "File ended inside scope",
                AnnotationType::Error,
sources,
&[(scope_start, "Closest scope starts here", None)]
            ),
            // TODO improve this error in the case that there was preceding code that meant to be a block scope/inline scope but wasn't.
            BlockScopeOpenedMidPara { scope_start } => snippet_from_spans(

                "Block scope (a scope directly followed by a newline) was opened inside a paragraph",
                AnnotationType::Error,
sources,
&[(scope_start, "Scope opened here", None)]
            ),
            BlockOwnerCodeMidPara { code_span } => snippet_from_spans(

                "A `BlockScopeOwner` was returned by inline code inside a paragraph",
                AnnotationType::Error,
sources,
&[(code_span, "BlockScopeOwner returned by this", None)]
            ),
            BlockCodeMidPara { code_span } => snippet_from_spans(

                "A `Block` was returned by inline code inside a paragraph",
                AnnotationType::Error,
sources,
&[(code_span, "Block returned by this", None)]
            ),
            InsertedFileMidPara { code_span } => snippet_from_spans(

                "A `TurnipTextSource` file was returned by inline code inside a paragraph",
                AnnotationType::Error,
sources,
&[(code_span, "TurnipTextSource returned by this", None)]
            ),
            BlockCodeFromRawScopeMidPara { code_span } => snippet_from_spans(

                "A `Block` was returned after building a raw scope inside a paragraph",
                AnnotationType::Error,
sources,
&[(code_span, "RawScopeBuilder returned by this", None)]
            ),
            SentenceBreakInInlineScope { scope_start } => snippet_from_spans(
 
                "Paragraph break found inside an inline scope",
                AnnotationType::Error,
                sources,
                &[(scope_start, "Inline scope opened here", None)],
                
            ),
            ParaBreakInInlineScope {
                scope_start,
                para_break
            } => Snippet {
                title: Some(Annotation {
                    label: Some("Paragraph break found inside an inline scope"),
                    id: None,
                    annotation_type: AnnotationType::Error,
                }),
                footer: vec![Annotation {
                    label: Some("An inline scope is for inline formatting only. Try removing the paragraph break, or moving the scope into its own block and putting a newline after the start to make it a block scope."),
                    id: None,
                    annotation_type: AnnotationType::Help
                }],
                slices: slices_from_spans(AnnotationType::Note, sources, &[
                    (scope_start, "Scope starts here", None),
                    (para_break, "Paragraph break here", Some(AnnotationType::Error))
                ]),
                opt: Default::default(),
            },
            BlockOwnerCodeHasNoScope { code_span } => snippet_from_spans(

                "`BlockScopeOwner` returned by inline code has no corresponding block scope",
                AnnotationType::Error,
sources,
&[(code_span, "BlockScopeOwner returned by this", None)]
            ),
            InlineOwnerCodeHasNoScope { code_span } => snippet_from_spans(

                "`InlineScopeOwner` returned by inline code has no corresponding inline scope",
                AnnotationType::Error,
sources,
&[(code_span, "InlineScopeOwner returned by this", None)]
            ),
            PythonErr { ctx, pyerr, code_span } => Snippet {
                title: Some(Annotation {
                    label: Some("Error executing user-defined Python"),
                    id: None,
                    annotation_type: AnnotationType::Error
                }),
                footer: vec![Annotation {
                    label: Some(ctx.as_str()),
                    id: None,
                    annotation_type: AnnotationType::Note
                }, Annotation{
                    label: Some(pyerr.as_str()),
                    id: None,
                    annotation_type: AnnotationType::Error
                }],
                slices: slices_from_spans(AnnotationType::Note, sources, &[
                    (code_span, "Code executed here", None),
                ]),
                opt: Default::default()
            },
            EscapedNewlineOutsideParagraph { newline } => snippet_from_spans(

                "A backslash-escaped newline, which means 'continue the sentence', was found outside a paragraph with no sentence to continue.",
                AnnotationType::Error,
                sources,
                &[(newline, "Backslash-escaped newline here", None)],
            ),
            DocSegmentHeaderMidPara { code_span } => snippet_from_spans(

                "An eval-bracket returned a DocSegmentHeader inside a Paragraph. DocSegmentHeaders are block-level only.",
                AnnotationType::Error,
                sources,
                &[(code_span, "Eval-bracket here", None)],
            ),
            DocSegmentHeaderMidScope { code_span, block_close_span, enclosing_scope_start } => {
                match block_close_span {
                    Some(block_close_span) => {
                        Snippet {
                            title: Some(Annotation {
                                label: Some("A BlockScopeBuilder inside a block scope returned a DocSegmentHeader."),
                                id: None,
                                annotation_type: AnnotationType::Error
                            }),
                            footer: vec![Annotation{
                                label: Some("DocSegmentHeaders are only allowed at the top level"),
                                id: None,
                                annotation_type: AnnotationType::Error
                            }],
                            slices: slices_from_spans(AnnotationType::Note, sources, &[
                                (code_span, "BlockScopeBuilder created here", None),
                                (block_close_span, "Block closed here, calling build_from_blocks", None),
                                (enclosing_scope_start, "Enclosing scope starts here", None)
                            ]),
                            opt: Default::default()
                        }
                    }
                    None => {
                        Snippet {
                            title: Some(Annotation {
                                label: Some("An eval-bracket inside a block scope returned a DocSegmentHeader."),
                                id: None,
                                annotation_type: AnnotationType::Error
                            }),
                            footer: vec![Annotation{
                                label: Some("DocSegmentHeaders are only allowed at the top level"),
                                id: None,
                                annotation_type: AnnotationType::Error
                            }],
                            slices: slices_from_spans(AnnotationType::Note, sources, &[
                                (code_span, "Code executed here", None),
                                (enclosing_scope_start, "Enclosing scope starts here", None),
                            ]),
                            opt: Default::default()
                        }
                    }
                }
            },
            InsufficientBlockSeparation { last_block, next_block_start } => {
                Snippet {
                    title: Some(Annotation {
                        label: Some("Insufficient separation between the end of a block and the start of a new one."),
                        id: None,
                        annotation_type: AnnotationType::Error
                    }),
                    footer: vec![Annotation{
                        label: Some("Blocks must be visually separated in turnip-text code."),
                        id: None,
                        annotation_type: AnnotationType::Note
                    }],
                    slices: slices_from_spans(AnnotationType::Note, sources, &[
                        (last_block, "A Block was created and concluded...", None),
                        (next_block_start, "...then on the same line a new block was started.", None),
                    ]),
                    opt: Default::default()
                }
            }
        }
    }
}