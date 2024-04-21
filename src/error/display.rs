//! This module tends to have a lot of long strings which cargo fmt won't automatically handle.
//! `rustfmt .\src\error.rs --config format_strings=true` is good for this.

use annotate_snippets::snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation};

use crate::{interpreter::ParsingFile, lexer::LexError, util::ParseSpan};

use super::{interp::InterpError, TurnipTextError, UserPythonExecError};

fn snippet_from_spans<'a>(
    top_label: &'a str,
    annotation_type: AnnotationType,
    sources: &'a Vec<ParsingFile>,
    spans: &[(&'a ParseSpan, &'a str, Option<AnnotationType>)],
    hint: &'a str,
) -> Snippet<'a> {
    Snippet {
        title: Some(Annotation {
            label: Some(top_label),
            id: None,
            annotation_type,
        }),
        footer: vec![Annotation {
            id: None,
            label: Some(hint),
            annotation_type: AnnotationType::Help,
        }],
        slices: slices_from_spans(annotation_type, sources, spans),
        opt: Default::default(),
    }
}

fn slices_from_spans<'a>(
    default_annotation_type: AnnotationType,
    sources: &'a Vec<ParsingFile>,
    spans: &[(&ParseSpan, &'a str, Option<AnnotationType>)],
) -> Vec<Slice<'a>> {
    spans
        .iter()
        .map(|(span, label, annotation_type)| {
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
        })
        .collect()
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
    pub fn snippet<'a>(&'a self) -> Snippet<'a> {
        match self {
            TurnipTextError::Lex(sources, file_idx, err) => {
                Self::lex_error_snippet(sources, *file_idx, err)
            }
            TurnipTextError::Interp(sources, err) => Self::interp_error_snippet(sources, err),
            TurnipTextError::UserPython(sources, err) => Self::user_python_snippet(sources, err),
            TurnipTextError::InternalPython(pyerr) => Snippet {
                title: Some(Annotation {
                    label: Some("Internal Python error"),
                    id: None,
                    annotation_type: AnnotationType::Error,
                }),
                footer: vec![Annotation {
                    label: Some(&pyerr),
                    id: None,
                    annotation_type: AnnotationType::Error,
                }],
                slices: vec![],
                opt: Default::default(),
            },
        }
    }

    fn lex_error_snippet<'a>(
        sources: &'a Vec<ParsingFile>,
        file_idx: usize,
        err: &'a LexError,
    ) -> Snippet<'a> {
        // TODO - in the event this breaks on non-ASCII/non-single-byte,
        // it would be nice to print the character somewhere
        Snippet {
            title: Some(Annotation {
                label: Some("Parser error"),
                id: None,
                annotation_type: AnnotationType::Error,
            }),
            footer: vec![],
            slices: slices_from_spans(
                AnnotationType::Error,
                sources,
                &[(
                    &ParseSpan::single_char(file_idx, err.pos, err.ch),
                    "Unexpected character",
                    None,
                )],
            ),
            opt: Default::default(),
        }
    }

    fn interp_error_snippet<'a>(
        sources: &'a Vec<ParsingFile>,
        err: &'a Box<InterpError>,
    ) -> Snippet<'a> {
        todo!("uncomment");
        use InterpError::*;
        /*
        match err.as_ref() {
            CodeCloseOutsideCode(span) => snippet_from_spans(
                "Code close token in text mode",
                AnnotationType::Error,
                sources,
                &[(span, "", None)],
                "If you want to use ']' in text, try escaping it with a backslash '\\]'",
            ),
            BlockScopeCloseOutsideScope(span) => snippet_from_spans(
                "Encountered a scope close token at the block level when this file didn't have \
                 any open block scopes",
                AnnotationType::Error,
                sources,
                &[(span, "", None)],
                "If you want to use '}' in text, try escaping it with a backslash '\\]'",
            ),
            InlineScopeCloseOutsideScope(span) => snippet_from_spans(
                "Encountered a scope close token in inline mode when there weren't any open \
                 inline scopes",
                AnnotationType::Error,
                sources,
                &[(span, "", None)],
                "If you want to use '}' in text, try escaping it with a backslash '\\]'",
            ),
            RawScopeCloseOutsideRawScope(span) => snippet_from_spans(
                "Raw scope close token when outside scope",
                AnnotationType::Error,
                sources,
                &[(span, "", None)],
                "If you want to use '}###' etc. in text, try escaping each character with a \
                 backslash '\\}\\#\\#\\#' etc.",
            ),
            EndedInsideCode { code_start } => snippet_from_spans(
                "File ended inside code block",
                AnnotationType::Error,
                sources,
                &[(code_start, "Code block starts here", None)],
                "Make sure to close code mode! If you meant to use '[' in text instead of \
                 starting code, try escaping it with a backslash '\\['",
            ),
            EndedInsideRawScope { raw_scope_start } => snippet_from_spans(
                "File ended inside raw scope",
                AnnotationType::Error,
                sources,
                &[(raw_scope_start, "Raw scope starts here", None)],
                "Make sure to close any scopes you open! If you meant to use '###{' in text \
                 instead of opening a scope, try escaping each character with a backslash \
                 '\\#\\#\\#\\{'",
            ),
            EndedInsideScope { scope_start } => snippet_from_spans(
                "File ended inside scope",
                AnnotationType::Error,
                sources,
                &[(scope_start, "Closest scope starts here", None)],
                "Make sure to close any scopes you open! If you meant to use '{' in text instead \
                 of opening a scope, try escaping it with a backslash '\\{'",
            ),
            BlockScopeOpenedMidPara { scope_start } => snippet_from_spans(
                "Block scope (a scope followed by a newline) was opened inside a paragraph",
                AnnotationType::Error,
                sources,
                &[(scope_start, "Scope opened here", None)],
                "Blocks can't be emitted inside Paragraphs. Try opening this scope on a new line, \
                 with a blank line to separate it from the paragraph.",
            ),
            BlockOwnerCodeMidPara { code_span } => snippet_from_spans(
                "A `BlockScopeOwner` was returned by inline code inside a paragraph",
                AnnotationType::Error,
                sources,
                &[(code_span, "BlockScopeOwner returned by this", None)],
                "TODO this isn't an error anymore lol",
            ),
            BlockCodeMidPara { code_span } => snippet_from_spans(
                "A `Block` was returned by inline code inside a paragraph",
                AnnotationType::Error,
                sources,
                &[(code_span, "Block returned by this", None)],
                "Blocks can't be emitted inside Paragraphs. Try placing this code on a new line, \
                 with a blank line to separate it from the paragraph.",
            ),
            InsertedFileMidPara { code_span } => snippet_from_spans(
                "A `TurnipTextSource` file was returned by inline code inside a paragraph",
                AnnotationType::Error,
                sources,
                &[(code_span, "TurnipTextSource returned by this", None)],
                "We can't enter a new source file when we're in inline mode - either inside a \
                 paragraph or an inline scope.",
            ),
            BlockCodeFromRawScopeMidPara { code_span } => snippet_from_spans(
                "A `Block` was returned after building a raw scope inside a paragraph",
                AnnotationType::Error,
                sources,
                &[(code_span, "RawScopeBuilder returned by this", None)],
                "Blocks can't be emitted inside Paragraphs. Try placing this code on a new line, \
                 with a blank line to separate it from the paragraph.",
            ),
            SentenceBreakInInlineScope { scope_start } => snippet_from_spans(
                "Paragraph break found inside an inline scope",
                AnnotationType::Error,
                sources,
                &[(scope_start, "Inline scope opened here", None)],
                "You can't start a new paragraph inside an inline scope. Try closing the inline \
                 scope first with '}', or make it a block scope by opening a newline directly \
                 after the opening squiggly brace.",
            ),
            ParaBreakInInlineScope {
                scope_start,
                para_break,
            } => Snippet {
                title: Some(Annotation {
                    label: Some("Paragraph break found inside an inline scope"),
                    id: None,
                    annotation_type: AnnotationType::Error,
                }),
                footer: vec![Annotation {
                    label: Some(
                        "An inline scope is for inline formatting only.Try removing the paragraph \
                         break, or moving the scope into its own block and putting a newline \
                         after the start to make it a block scope",
                    ),
                    id: None,
                    annotation_type: AnnotationType::Help,
                }],
                slices: slices_from_spans(
                    AnnotationType::Note,
                    sources,
                    &[
                        (scope_start, "Scope starts here", None),
                        (
                            para_break,
                            "Paragraph break here",
                            Some(AnnotationType::Error),
                        ),
                    ],
                ),
                opt: Default::default(),
            },
            BlockOwnerCodeHasNoScope { code_span } => snippet_from_spans(
                "`BlockScopeOwner` returned by inline code has no corresponding block scope",
                AnnotationType::Error,
                sources,
                &[(code_span, "BlockScopeOwner returned by this", None)],
                "Try opening a block scope directly after this code using squiggly braces {\\n} \
                 with a newline directly after the opening brace.",
            ),
            InlineOwnerCodeHasNoScope { code_span } => snippet_from_spans(
                "`InlineScopeOwner` returned by inline code has no corresponding inline scope",
                AnnotationType::Error,
                sources,
                &[(code_span, "InlineScopeOwner returned by this", None)],
                "Try opening an inline scope directly after this code using squiggly braces {} \
                 with no newlines inside",
            ),
            PythonErr {
                ctx,
                pyerr,
                code_span,
            } => Snippet {
                title: Some(Annotation {
                    label: Some("Error executing user-defined Python"),
                    id: None,
                    annotation_type: AnnotationType::Error,
                }),
                footer: vec![
                    Annotation {
                        label: Some(ctx.as_str()),
                        id: None,
                        annotation_type: AnnotationType::Note,
                    },
                    Annotation {
                        label: Some(pyerr.as_str()),
                        id: None,
                        annotation_type: AnnotationType::Error,
                    },
                ],
                slices: slices_from_spans(
                    AnnotationType::Note,
                    sources,
                    &[(code_span, "Code executed here", None)],
                ),
                opt: Default::default(),
            },
            EscapedNewlineOutsideParagraph { newline } => snippet_from_spans(
                "A backslash-escaped newline, which means 'continue the sentence', was found \
                 outside a paragraph with no sentence to continue.",
                AnnotationType::Error,
                sources,
                &[(newline, "Backslash-escaped newline here", None)],
                "Delete the backslash to remove this error. Newlines are only relevant inside \
                 comments and inline mode, you don't need to escape them anywhere else.",
            ),
            DocSegmentHeaderMidPara { code_span } => snippet_from_spans(
                "An eval-bracket returned a DocSegmentHeader inside a Paragraph. \
                 DocSegmentHeaders are block-level only.",
                AnnotationType::Error,
                sources,
                &[(code_span, "Eval-bracket here", None)],
                "Make sure to separate any code emitting DocSegmentHeader from other content with \
                 blank lines.",
            ),
            DocSegmentHeaderMidScope {
                code_span,
                block_close_span,
                enclosing_scope_start,
            } => match block_close_span {
                Some(block_close_span) => Snippet {
                    title: Some(Annotation {
                        label: Some(
                            "A BlockScopeBuilder inside a block scope returned a DocSegmentHeader.",
                        ),
                        id: None,
                        annotation_type: AnnotationType::Error,
                    }),
                    footer: vec![Annotation {
                        label: Some("DocSegmentHeaders are only allowed at the top level"),
                        id: None,
                        annotation_type: AnnotationType::Error,
                    }],
                    slices: slices_from_spans(
                        AnnotationType::Note,
                        sources,
                        &[
                            (code_span, "BlockScopeBuilder created here", None),
                            (
                                block_close_span,
                                "Block closed here, calling build_from_blocks",
                                None,
                            ),
                            (enclosing_scope_start, "Enclosing scope starts here", None),
                        ],
                    ),
                    opt: Default::default(),
                },
                None => Snippet {
                    title: Some(Annotation {
                        label: Some(
                            "An eval-bracket inside a block scope returned a DocSegmentHeader.",
                        ),
                        id: None,
                        annotation_type: AnnotationType::Error,
                    }),
                    footer: vec![Annotation {
                        label: Some("DocSegmentHeaders are only allowed at the top level"),
                        id: None,
                        annotation_type: AnnotationType::Error,
                    }],
                    slices: slices_from_spans(
                        AnnotationType::Note,
                        sources,
                        &[
                            (code_span, "Code executed here", None),
                            (enclosing_scope_start, "Enclosing scope starts here", None),
                        ],
                    ),
                    opt: Default::default(),
                },
            },
            // TODO update error message if the last_block was a file or not.
            InsufficientBlockSeparation {
                last_block,
                next_block_start,
            } => Snippet {
                title: Some(Annotation {
                    label: Some(
                        "Insufficient separation between the end of a block and the start of a \
                         new one.",
                    ),
                    id: None,
                    annotation_type: AnnotationType::Error,
                }),
                footer: vec![Annotation {
                    label: Some(
                        "Blocks must be visually separated in turnip-text code. Start the new \
                         block on a new line.",
                    ),
                    id: None,
                    annotation_type: AnnotationType::Note,
                }],
                slices: slices_from_spans(
                    AnnotationType::Note,
                    sources,
                    &[
                        (last_block, "A Block was created and concluded...", None),
                        (
                            next_block_start,
                            "...then on the same line a new block was started.",
                            None,
                        ),
                    ],
                ),
                opt: Default::default(),
            },
            InsufficientParaNewBlockOrFileSeparation {
                para,
                next_block_start,
                was_block_not_file,
            } => Snippet {
                title: Some(Annotation {
                    label: Some(if *was_block_not_file {
                        "Insufficient separation between the end of a paragraph and a new block."
                    } else {
                        "Insufficient separation between the end of a paragraph and an emitted \
                         TurnipTextSource."
                    }),
                    id: None,
                    annotation_type: AnnotationType::Error,
                }),
                footer: vec![Annotation {
                    label: Some(if *was_block_not_file {
                        "Blocks must be visually separated in turnip-text code. Add a blank line \
                         between the paragraph and the new block."
                    } else {
                        "TurnipTextSource files are emitted at the block level and must be \
                         visually separated from paragraphs. Add a blank line between the \
                         paragraph and the code emitting the file."
                    }),
                    id: None,
                    annotation_type: AnnotationType::Note,
                }],
                slices: slices_from_spans(
                    AnnotationType::Note,
                    sources,
                    &[
                        (para, "The paragraph was created and concluded...", None),
                        (
                            next_block_start,
                            if *was_block_not_file {
                                "...then on the next line a new block was created."
                            } else {
                                "...then on the next line a TurnipTextSource was emitted in code."
                            },
                            None,
                        ),
                    ],
                ),
                opt: Default::default(),
            },
        }*/
    }

    fn user_python_snippet<'a>(
        sources: &'a Vec<ParsingFile>,
        error: &Box<UserPythonExecError>,
    ) -> Snippet<'a> {
        todo!()
    }
}
