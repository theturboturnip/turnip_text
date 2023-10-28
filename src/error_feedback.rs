use annotate_snippets::{display_list::DisplayList, snippet::*};
use lexer_rs::PosnInCharStream;

use crate::{
    lexer::LexError,
    util::ParseSpan, python::InterpError,
};

pub trait GivesCliFeedback {
    fn get_snippet<'a>(&'a self, file_src: &'a str) -> Snippet<'a>;
}
impl GivesCliFeedback for LexError {
    fn get_snippet<'a>(&'a self, file_src: &'a str) -> Snippet<'a> {
        // TODO - in the event this breaks on non-ASCII/non-single-byte,
        // it would be nice to print the character somewhere
        Snippet {
            title: Some(Annotation {
                label: Some("Parser error"),
                id: None,
                annotation_type: AnnotationType::Error,
            }),
            footer: vec![],
            slices: vec![Slice {
                source: file_src,
                line_start: 1,
                origin: None,
                fold: true,
                annotations: vec![SourceAnnotation {
                    range: (self.pos.byte_ofs(), self.pos.byte_ofs() + self.ch.len_utf8()),
                    label: "Unexpected character",
                    annotation_type: AnnotationType::Error,
                }],
            }],
            opt: Default::default(),
        }
    }
}

fn snippet_from_parse_span<'a>(
    file_src: &'a str,
    top_label: &'a str,
    specific_label: &'a str,
    annotation_type: AnnotationType,
    span: &ParseSpan,
) -> Snippet<'a> {
    Snippet {
        title: Some(Annotation {
            label: Some(top_label),
            id: None,
            annotation_type,
        }),
        footer: vec![],
        slices: vec![Slice {
            source: file_src,
            line_start: 1,
            origin: None,
            fold: true,
            annotations: vec![annotation_from_parse_span(
                specific_label,
                annotation_type,
                span,
            )],
        }],
        opt: Default::default(),
    }
}
fn annotation_from_parse_span<'a>(
    label: &'a str,
    annotation_type: AnnotationType,
    span: &ParseSpan,
) -> SourceAnnotation<'a> {
    SourceAnnotation {
        range: (span.start.char_ofs, span.end.char_ofs),
        label,
        annotation_type,
    }
}

impl GivesCliFeedback for InterpError {
    fn get_snippet<'a>(&'a self, file_src: &'a str) -> Snippet<'a> {
        use InterpError::*;
        match self {
            CodeCloseOutsideCode(span) => snippet_from_parse_span(
                file_src,
                "Code close token in text mode",
                "",
                AnnotationType::Error,
                span,
            ),
            ScopeCloseOutsideScope(span) => snippet_from_parse_span(
                file_src,
                "Scope close token when outside scope",
                "",
                AnnotationType::Error,
                span,
            ),
            RawScopeCloseOutsideRawScope(span) => snippet_from_parse_span(
                file_src,
                "Raw scope close token when outside scope",
                "",
                AnnotationType::Error,
                span,
            ),
            EndedInsideCode { code_start } => snippet_from_parse_span(
                file_src,
                "File ended inside code block",
                "Code block starts here",
                AnnotationType::Error,
                code_start,
            ),
            EndedInsideRawScope { raw_scope_start } => snippet_from_parse_span(
                file_src,
                "File ended inside raw scope",
                "Raw scope starts here",
                AnnotationType::Error,
                raw_scope_start,
            ),
            EndedInsideScope { scope_start } => snippet_from_parse_span(
                file_src,
                "File ended inside scope",
                "Closest scope starts here",
                AnnotationType::Error,
                scope_start,
            ),
            // TODO improve this error in the case that there was preceding code that meant to be a block scope/inline scope but wasn't.
            BlockScopeOpenedMidPara { scope_start } => snippet_from_parse_span(
                file_src,
                "Block scope (a scope directly followed by a newline) was opened inside a paragraph",
                "Scope opened here",
                AnnotationType::Error,
                scope_start,
            ),
            BlockOwnerCodeMidPara { code_span } => snippet_from_parse_span(
                file_src,
                "A `BlockScopeOwner` was returned by inline code inside a paragraph",
                "BlockScopeOwner returned by this",
                AnnotationType::Error,
                code_span,
            ),
            BlockCodeMidPara { code_span } => snippet_from_parse_span(
                file_src,
                "A `Block` was returned by inline code inside a paragraph",
                "Block returned by this",
                AnnotationType::Error,
                code_span,
            ),
            BlockCodeFromRawScopeMidPara { code_span } => snippet_from_parse_span(
                file_src,
                "A `Block` was returned after building a raw scope inside a paragraph",
                "RawScopeBuilder returned by this",
                AnnotationType::Error,
                code_span,
            ),
            SentenceBreakInInlineScope { scope_start } => snippet_from_parse_span(
                file_src, 
                "Paragraph break found inside an inline scope",
                "Inline scope opened here",
                AnnotationType::Error,
                scope_start
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
                slices: vec![Slice {
                    source: file_src,
                    line_start: 1,
                    origin: None,
                    fold: true,
                    annotations: vec![
                        annotation_from_parse_span(
                            "Scope starts here",
                            AnnotationType::Note,
                            scope_start,
                        ),
                        annotation_from_parse_span(
                            "Paragraph break here",
                            AnnotationType::Error,
                            para_break,
                        ),
                    ],
                }],
                opt: Default::default(),
            },
            BlockOwnerCodeHasNoScope { code_span } => snippet_from_parse_span(
                file_src,
                "`BlockScopeOwner` returned by inline code has no corresponding block scope",
                "BlockScopeOwner returned by this",
                AnnotationType::Error,
                code_span,
            ),
            InlineOwnerCodeHasNoScope { code_span } => snippet_from_parse_span(
                file_src,
                "`InlineScopeOwner` returned by inline code has no corresponding inline scope",
                "InlineScopeOwner returned by this",
                AnnotationType::Error,
                code_span,
            ),
            PythonErr { pyerr, code_span } => Snippet {
                title: Some(Annotation {
                    label: Some("Error executing user-defined Python"),
                    id: None,
                    annotation_type: AnnotationType::Error
                }),
                footer: vec![Annotation{
                    label: Some(pyerr.as_str()),
                    id: None,
                    annotation_type: AnnotationType::Error
                }],
                slices: vec![Slice {
                    source: file_src,
                    line_start: 1,
                    origin: None,
                    fold: true,
                    annotations: vec![
                        annotation_from_parse_span(
                            "Code executed here",
                            AnnotationType::Note,
                            code_span,
                        ),
                    ],
                }],
                opt: Default::default()
            },
            InternalPythonErr { pyerr } => Snippet {
                title: Some(Annotation {
                    label: Some("Internal Python error"),
                    id: None,
                    annotation_type: AnnotationType::Error
                }),
                footer: vec![Annotation{
                    label: Some(pyerr.as_str()),
                    id: None,
                    annotation_type: AnnotationType::Error
                }],
                slices: vec![],
                opt: Default::default(),
            },
            InternalErr(err) => Snippet {
                title: Some(Annotation {
                    label: Some("Internal error"),
                    id: None,
                    annotation_type: AnnotationType::Error
                }),
                footer: vec![Annotation{
                    label: Some(err.as_str()),
                    id: None,
                    annotation_type: AnnotationType::Error
                }],
                slices: vec![],
                opt: Default::default(),
            },
            EscapedNewlineOutsideParagraph { newline } => snippet_from_parse_span(
                file_src,
                "A backslash-escaped newline, which means 'continue the sentence', was found outside a paragraph with no sentence to continue.",
                "Backslash-escaped newline here",
                AnnotationType::Error,
                newline
            ),
            DocSegmentHeaderMidPara { code_span } => snippet_from_parse_span(
                file_src,
                "An eval-bracket returned a DocSegmentHeader inside a Paragraph. DocSegmentHeaders are block-level only.",
                "Eval-bracket here",
                AnnotationType::Error,
                code_span
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
                            slices: vec![Slice {
                                source: file_src,
                                line_start: 1,
                                origin: None,
                                fold: true,
                                annotations: vec![
                                    annotation_from_parse_span(
                                        "BlockScopeBuilder created here",
                                        AnnotationType::Note,
                                        code_span,
                                    ),
                                    annotation_from_parse_span(
                                        "Block closed here, calling build_from_blocks",
                                        AnnotationType::Note,
                                        block_close_span,
                                    ),
                                    annotation_from_parse_span(
                                        "Enclosing scope starts here",
                                        AnnotationType::Note,
                                        enclosing_scope_start
                                    )
                                ],
                            }],
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
                            slices: vec![Slice {
                                source: file_src,
                                line_start: 1,
                                origin: None,
                                fold: true,
                                annotations: vec![
                                    annotation_from_parse_span(
                                        "Code executed here",
                                        AnnotationType::Note,
                                        code_span,
                                    ),
                                    annotation_from_parse_span(
                                        "Enclosing scope starts here",
                                        AnnotationType::Note,
                                        enclosing_scope_start
                                    )
                                ],
                            }],
                            opt: Default::default()
                        }
                    }
                }
            }
        }
    }
}

fn display_cli_feedback<T: GivesCliFeedback>(data: &str, err: &T) {
    let dl = DisplayList::from(err.get_snippet(&data));
    eprintln!("{}", dl);
}