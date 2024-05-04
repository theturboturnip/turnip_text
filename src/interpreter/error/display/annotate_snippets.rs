//! This module tends to have a lot of long strings which cargo fmt won't automatically handle.
//! `rustfmt .\src\error.rs --config format_strings=true` is good for this.

/// TODO try https://github.com/brendanzab/codespan instead
/// Or just do it myself :P
use annotate_snippets::{
    display_list::DisplayList,
    snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation},
};

use crate::{
    interpreter::{error::user_python::UserPythonCompileMode, ParsingFile},
    util::ParseSpan,
};

use super::super::{
    syntax::{BlockModeElem, InlineModeContext, TTSyntaxError},
    TTErrorWithContext, TTUserPythonError,
};

fn snippet_from_spans<'a>(
    top_label: &'a str,
    annotation_type: AnnotationType,
    sources: &'a Vec<ParsingFile>,
    spans: &[(ParseSpan, &'a str, Option<AnnotationType>)],
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
    spans: &[(ParseSpan, &'a str, Option<AnnotationType>)],
) -> Vec<Slice<'a>> {
    let mut slices = vec![];
    let mut curr_slice = None;
    let mut prev_file_idx = None;
    for (span, label, annotation_type) in spans {
        if prev_file_idx != Some(span.file_idx()) {
            curr_slice.map(|slice| slices.push(slice));
            let file = &sources[span.file_idx()];
            curr_slice = Some(Slice {
                origin: Some(file.name()),
                source: file.contents(),
                line_start: 1,
                fold: true,
                annotations: vec![],
            });
            prev_file_idx = Some(span.file_idx());
        }

        curr_slice
            .as_mut()
            .unwrap()
            .annotations
            .push(annotation_from_parse_span(
                *label,
                annotation_type.unwrap_or(default_annotation_type),
                &span,
            ));
    }
    curr_slice.map(|slice| slices.push(slice));
    slices
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

pub fn detailed_message_of(err: &TTErrorWithContext) -> String {
    format!("{}", DisplayList::from(err.snippet()))
}

impl TTErrorWithContext {
    fn snippet<'a>(&'a self) -> Snippet<'a> {
        match self {
            TTErrorWithContext::NullByteFoundInSource { source_name } => Snippet {
                title: Some(Annotation {
                    label: Some("Found a null byte '\\0' in a TurnipTextSource string, which isn't allowed. This source is probably corrupted, not a text file, or was read with the wrong encoding."),
                    id: None,
                    annotation_type: AnnotationType::Error,
                }),
                footer: vec![Annotation {
                    label: Some(&source_name),
                    id: None,
                    annotation_type: AnnotationType::Error,
                }],
                slices: vec![],
                opt: Default::default(),
            },
            TTErrorWithContext::Syntax(sources, err) => Self::interp_error_snippet(sources, err),
            TTErrorWithContext::UserPython(sources, err) => Self::user_python_snippet(sources, err),
            TTErrorWithContext::InternalPython(_pyerr) => Snippet {
                title: Some(Annotation {
                    label: Some("Internal Python error. Please file a bug report at https://github.com/theturboturnip/turnip_text with the traceback and an example document."),
                    id: None,
                    annotation_type: AnnotationType::Error,
                }),
                // We don't stringify the PyErr here because this snippet is just context to the actual reraised Python error
                // FUTURE: If running this as a CLI tool with Python inside, this snippet is the only debug info and it would be good to stringify the PyErr.
                footer: vec![],
                slices: vec![],
                opt: Default::default(),
            },
        }
    }

    fn push_inline_mode_ctx_note<'a>(
        inl_mode: &'a InlineModeContext,
        notes: &mut Vec<(ParseSpan, &'a str, Option<AnnotationType>)>,
    ) {
        match inl_mode {
            InlineModeContext::Paragraph(c) => {
                notes.push((
                    c.first_tok(),
                    "Started paragraph here...",
                    Some(AnnotationType::Note),
                ));
                notes.push((
                    c.last_tok(),
                    "...still in inline mode here...",
                    Some(AnnotationType::Note),
                ));
            }
            InlineModeContext::InlineScope { scope_start } => notes.push((
                *scope_start,
                "Inline scope started here...",
                Some(AnnotationType::Note),
            )),
        }
    }
    fn interp_error_snippet<'a>(
        sources: &'a Vec<ParsingFile>,
        err: &'a Box<TTSyntaxError>,
    ) -> Snippet<'a> {
        use TTSyntaxError::*;
        match err.as_ref() {
            CodeCloseOutsideCode(span) => snippet_from_spans(
                "Code close token in text mode",
                AnnotationType::Error,
                sources,
                &[(*span, "", None)],
                "If you want to use ']' in text, try escaping it with a backslash '\\]'",
            ),
            BlockScopeCloseOutsideScope(span) => snippet_from_spans(
                "Encountered a scope close token at the block level when this file didn't have \
                 any open block scopes",
                AnnotationType::Error,
                sources,
                &[(*span, "", None)],
                "If you want to use '}' in text, try escaping it with a backslash '\\]'",
            ),
            InlineScopeCloseOutsideScope(span) => snippet_from_spans(
                "Encountered a scope close token in inline mode when there weren't any open \
                 inline scopes",
                AnnotationType::Error,
                sources,
                &[(*span, "", None)],
                "If you want to use '}' in text, try escaping it with a backslash '\\]'",
            ),
            RawScopeCloseOutsideRawScope(span) => snippet_from_spans(
                "Raw scope close token when outside scope",
                AnnotationType::Error,
                sources,
                &[(*span, "", None)],
                "If you want to use '}###' etc. in text, try escaping each character with a \
                 backslash '\\}\\#\\#\\#' etc.",
            ),
            EndedInsideCode {
                code_start,
                eof_span,
            } => snippet_from_spans(
                "File ended inside code block",
                AnnotationType::Error,
                sources,
                &[
                    (*code_start, "Code block starts here", None),
                    (*eof_span, "File ends here", None),
                ],
                "Make sure to close code mode! If you meant to use '[' in text instead of \
                 starting code, try escaping it with a backslash '\\['",
            ),
            EndedInsideRawScope {
                raw_scope_start,
                eof_span,
            } => snippet_from_spans(
                "File ended inside raw scope",
                AnnotationType::Error,
                sources,
                &[
                    (*raw_scope_start, "Raw scope starts here", None),
                    (*eof_span, "File ends here", None),
                ],
                "Make sure to close any scopes you open! If you meant to use '###{' in text \
                 instead of opening a scope, try escaping each character with a backslash \
                 '\\#\\#\\#\\{'",
            ),
            EndedInsideScope {
                scope_start,
                eof_span,
            } => snippet_from_spans(
                "File ended inside scope",
                AnnotationType::Error,
                sources,
                &[
                    (*scope_start, "Closest scope starts here", None),
                    (*eof_span, "File ends here", None),
                ],
                "Make sure to close any scopes you open! If you meant to use '{' in text instead \
                 of opening a scope, try escaping it with a backslash '\\{'",
            ),
            BlockScopeOpenedInInlineMode {
                inl_mode,
                block_scope_open,
            } => {
                let mut notes = vec![];
                Self::push_inline_mode_ctx_note(inl_mode, &mut notes);
                notes.push((
                    *block_scope_open,
                    "Block scope opened here",
                    Some(AnnotationType::Error),
                ));
                snippet_from_spans(
                    "Block scope (a scope followed by a newline) was opened in inline mode",
                    AnnotationType::Error,
                    sources,
                    &notes,
                    "Blocks can't be emitted in inline mode. Try opening this scope on a new \
                     line, with a blank line to separate it from the paragraph.",
                )
            }
            CodeEmittedBlockInInlineMode {
                inl_mode,
                block,
                code_span,
            } => {
                // TODO attach info about the block
                let mut notes = vec![];
                Self::push_inline_mode_ctx_note(inl_mode, &mut notes);
                notes.push((
                    *code_span,
                    "Block emitted by code here",
                    Some(AnnotationType::Error),
                ));
                snippet_from_spans(
                    "A `Block` was returned by code in inline mode.",
                    AnnotationType::Error,
                    sources,
                    &notes,
                    "Blocks can't be emitted in inline mode. Try opening this scope on a new \
                     line, with a blank line to separate it from the paragraph.",
                )
            }
            CodeEmittedHeaderInInlineMode {
                inl_mode,
                header,
                code_span,
            } => {
                // TODO attach info about the header
                let mut notes = vec![];
                Self::push_inline_mode_ctx_note(inl_mode, &mut notes);
                notes.push((*code_span, "Eval-bracket here", None));
                snippet_from_spans(
                    "A `Header` was returned by code in inline mode. Headers are block-level only.",
                    AnnotationType::Error,
                    sources,
                    &notes,
                    "Make sure to separate any code emitting Header from other content with blank \
                     lines.",
                )
            }
            CodeEmittedHeaderInBlockScope {
                block_scope_start,
                header,
                code_span,
            } => Snippet {
                // TODO attach info about the header
                title: Some(Annotation {
                    label: Some("Code inside a block scope returned a Header."),
                    id: None,
                    annotation_type: AnnotationType::Error,
                }),
                footer: vec![Annotation {
                    label: Some("Headers are only allowed at the top level."),
                    id: None,
                    annotation_type: AnnotationType::Error,
                }],
                slices: slices_from_spans(
                    AnnotationType::Note,
                    sources,
                    &[
                        (*block_scope_start, "Enclosing scope starts here", None),
                        (*code_span, "Code executed here", None),
                    ],
                ),
                opt: Default::default(),
            },
            CodeEmittedSourceInInlineMode {
                inl_mode,
                code_span,
            } => {
                let mut notes = vec![];
                Self::push_inline_mode_ctx_note(inl_mode, &mut notes);
                notes.push((*code_span, "TurnipTextSource returned by this", None));
                snippet_from_spans(
                    "A `TurnipTextSource` file was returned by inline code inside a paragraph",
                    AnnotationType::Error,
                    sources,
                    &notes,
                    "We can't enter a new source file when we're in inline mode - either inside a \
                     paragraph or an inline scope.",
                )
            }
            SentenceBreakInInlineScope {
                scope_start,
                sentence_break,
            } => snippet_from_spans(
                "Paragraph break found inside an inline scope",
                AnnotationType::Error,
                sources,
                &[
                    (*scope_start, "Inline scope opened here", None),
                    (*sentence_break, "Sentence break here", None),
                ],
                "You can't start a new paragraph inside an inline scope. Try closing the inline \
                 scope first with '}', or make it a block scope by opening a newline directly \
                 after the opening squiggly brace.",
            ),
            EscapedNewlineInBlockMode { newline } => snippet_from_spans(
                "A backslash-escaped newline, which means 'continue the sentence', was found \
                 outside a paragraph with no sentence to continue.",
                AnnotationType::Error,
                sources,
                &[(*newline, "Backslash-escaped newline here", None)],
                "Delete the backslash to remove this error. Newlines are only relevant inside \
                 comments and inline mode, you don't need to escape them anywhere else.",
            ),
            InsufficientBlockSeparation {
                last_block,
                next_block_start,
            } => {
                let mut notes = vec![];
                use BlockModeElem::*;
                match last_block {
                    HeaderFromCode(s) => notes.push((
                        *s,
                        "A Header was emitted here...",
                        Some(AnnotationType::Info),
                    )),
                    Para(c) => {
                        notes.push((
                            c.first_tok(),
                            "A Paragraph started here...",
                            Some(AnnotationType::Info),
                        ));
                        notes.push((
                            c.last_tok(),
                            "...and was still in progress here...",
                            Some(AnnotationType::Info),
                        ))
                    }
                    BlockScope(c) => {
                        notes.push((
                            c.first_tok(),
                            "A block scope started here...",
                            Some(AnnotationType::Info),
                        ));
                        notes.push((
                            c.last_tok(),
                            "...and ended here...",
                            Some(AnnotationType::Info),
                        ))
                    }
                    BlockFromCode(s) => notes.push((
                        *s,
                        "A Block was emitted here...",
                        Some(AnnotationType::Info),
                    )),
                    SourceFromCode(s) => notes.push((
                        *s,
                        "A new source file was emitted here...",
                        Some(AnnotationType::Info),
                    )),
                    AnyToken(s) => notes.push((
                        *s,
                        "Some Block was generated by this...",
                        Some(AnnotationType::Info),
                    )),
                };
                match next_block_start {
                    HeaderFromCode(s) => notes.push((
                        *s,
                        "...need a blank line before emitting a Header here.",
                        Some(AnnotationType::Error),
                    )),
                    Para(c) => {
                        notes.push((
                            c.first_tok(),
                            "...need a blank line before starting a Paragraph here.",
                            Some(AnnotationType::Error),
                        ))
                        // Don't care about the end of the paragraph
                    }
                    BlockScope(c) => {
                        notes.push((
                            c.first_tok(),
                            "...need a blank line before starting a block scope here.",
                            Some(AnnotationType::Error),
                        ))
                        // Don't care about the end of the block scope.
                    }
                    BlockFromCode(s) => notes.push((
                        *s,
                        "...need a blank line before emitting a block here.",
                        Some(AnnotationType::Error),
                    )),
                    SourceFromCode(s) => notes.push((
                        *s,
                        "...need a blank line before emitting a source file here.",
                        Some(AnnotationType::Error),
                    )),
                    AnyToken(s) => notes.push((
                        *s,
                        "...need a blank line before any other content.",
                        Some(AnnotationType::Error),
                    )),
                }
                Snippet {
                    title: Some(Annotation {
                        label: Some(
                            "Insufficient separation between the end of a block and the start of \
                             a new one.",
                        ),
                        id: None,
                        annotation_type: AnnotationType::Error,
                    }),
                    footer: vec![Annotation {
                        label: Some(
                            "Blocks must be visually separated in turnip-text code. Start the new \
                             block on a new line. Headers and TurnipTextSource are also \
                             block-level elements and must also be separated.",
                        ),
                        id: None,
                        annotation_type: AnnotationType::Note,
                    }],
                    slices: slices_from_spans(AnnotationType::Note, sources, &notes),
                    opt: Default::default(),
                }
            }
        }
    }

    fn user_python_snippet<'a>(
        sources: &'a Vec<ParsingFile>,
        err: &Box<TTUserPythonError>,
    ) -> Snippet<'a> {
        dbg!(err);
        match err.as_ref() {
            TTUserPythonError::CompilingEvalBrackets {
                code_ctx,
                code,
                mode,
                err: _,
            } => snippet_from_spans(
                "Error when compiling Python from eval-brackets",
                AnnotationType::Error,
                sources,
                &[(
                    code_ctx.full_span(),
                    match mode {
                        UserPythonCompileMode::EvalExpr => "Code compiled as a Python expression",
                        UserPythonCompileMode::ExecStmts => "Code compiled as Python statements",
                        UserPythonCompileMode::ExecIndentedStmts => {
                            "Code compiled as indented Python statements"
                        }
                    },
                    None,
                )],
                "TODO include code",
            ),
            TTUserPythonError::RunningEvalBrackets {
                code_ctx,
                code,
                mode,
                err: _,
            } => snippet_from_spans(
                "Python from eval-brackets raised an error",
                AnnotationType::Error,
                sources,
                &[(
                    code_ctx.full_span(),
                    match mode {
                        UserPythonCompileMode::EvalExpr => "Code compiled as a Python expression",
                        UserPythonCompileMode::ExecStmts => "Code compiled as Python statements",
                        UserPythonCompileMode::ExecIndentedStmts => {
                            "Code compiled as indented Python statements"
                        }
                    },
                    None,
                )],
                "TODO include code",
            ),
            TTUserPythonError::CoercingNonBuilderEvalBracket { code_ctx, obj } => {
                snippet_from_spans(
                    "Python code evaluated something that couldn't be emitted into the document",
                    AnnotationType::Error,
                    sources,
                    &[(code_ctx.full_span(), "Evaluated this", None)],
                    "TODO include obj",
                )
            }
            TTUserPythonError::CoercingBlockScopeBuilder {
                code_ctx,
                obj,
                err: _,
            } => snippet_from_spans(
                "Python code had a block scope attached but wasn't a BlockScopeBuilder",
                AnnotationType::Error,
                sources,
                &[(
                    code_ctx.full_span(),
                    "Evaluated this, it wasn't a BlockScopeBuilder",
                    None,
                )],
                "TODO include obj",
            ),
            TTUserPythonError::CoercingInlineScopeBuilder {
                code_ctx,
                obj,
                err: _,
            } => snippet_from_spans(
                "Python code had an inline scope attached but wasn't an InlineScopeBuilder",
                AnnotationType::Error,
                sources,
                &[(
                    code_ctx.full_span(),
                    "Evaluated this, it wasn't a InlineScopeBuilder",
                    None,
                )],
                "TODO include obj",
            ),
            TTUserPythonError::CoercingRawScopeBuilder {
                code_ctx,
                obj,
                err: _,
            } => snippet_from_spans(
                "Python code had a raw scope attached but wasn't a RawScopeBuilder",
                AnnotationType::Error,
                sources,
                &[(
                    code_ctx.full_span(),
                    "Evaluated this, it wasn't a RawScopeBuilder",
                    None,
                )],
                "TODO include obj",
            ),
        }
    }
}
