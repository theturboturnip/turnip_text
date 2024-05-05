use std::collections::HashMap;

use crate::{
    interpreter::{
        error::{
            syntax::{BlockModeElem, InlineModeContext, TTSyntaxError},
            user_python::{TTUserPythonError, UserPythonBuildMode, UserPythonCompileMode},
            TTErrorWithContext,
        },
        ParsingFile,
    },
    python::{
        interop::{
            Block, BlockScopeBuilder, Header, Inline, InlineScopeBuilder, RawScopeBuilder,
            TurnipTextSource,
        },
        typeclass::PyTypeclass,
        util::{get_docstring, get_name, stringify_py},
    },
    util::ParseSpan,
};
use codespan_reporting::{
    diagnostic::{Diagnostic, Label, LabelStyle, Severity},
    files::SimpleFiles,
    term::{emit, termcolor::Buffer, Config, DisplayStyle},
};
use pyo3::{
    exceptions::PySyntaxError,
    prelude::*,
    types::{PyFloat, PyLong, PyString},
};

// This uses codespan_reporting functions to split each file into lines.
// This function is not guaranteed to match how the lexer handles newlines, but it should be close enough.
// FUTURE integrate this with the lexer etc. to ensure the line numbers always match up.
fn files_of<'a>(sources: &'a Vec<ParsingFile>) -> SimpleFiles<&'a str, &'a str> {
    let mut files = SimpleFiles::new();
    for src in sources {
        files.add(src.name(), src.contents());
    }
    files
}

pub fn detailed_message_of(py: Python, err: &TTErrorWithContext) -> String {
    let mut config = Config::default();
    config.display_style = DisplayStyle::Rich;
    config.tab_width = 4;
    config.start_context_lines = 5;
    config.end_context_lines = 5;

    match err {
        TTErrorWithContext::NullByteFoundInSource { .. }
        | TTErrorWithContext::InternalPython(_) => format!("{}", err),
        TTErrorWithContext::FileStackExceededLimit { files, limit } => {
            // Look for recursion: check for repeated entries of the same file names in the stack
            let mut name_map = HashMap::new();
            for file in files {
                let key = file.name();
                name_map.insert(key, name_map.get(key).map_or(1usize, |value| value + 1));
            }
            let mut message =
                format!("Exceeded the TurnipTextSource stack limit of {limit} files.\n");
            // Remove the names of files that haven't repeated
            name_map.retain(|_, occurences| *occurences > 1);
            if name_map.is_empty() {
                message.push_str(
                    "No repeated names were detected.\n\
                It may be OK to increase the `max_file_depth` argument, \
                but it is more likely something has gone wrong.",
                )
            } else {
                message.push_str(
                    "Found the following repeated TurnipTextSource names in the stack:\n",
                );
                for (name, occurences) in name_map {
                    message = format!("{message}'{name}':\t{occurences} times\n");
                }
                message.push_str("One of these files is likely guilty of recursion.")
            }
            message
        }
        TTErrorWithContext::Syntax(sources, err) => {
            let mut text_buf = Buffer::no_color();
            emit(
                &mut text_buf,
                &config,
                &files_of(sources),
                &detailed_syntax_message(py, err.as_ref()),
            )
            .expect("Failed to create error message");
            String::from_utf8(text_buf.into_inner()).expect("Error message was not valid UTF-8")
        }
        TTErrorWithContext::UserPython(sources, err) => {
            let mut text_buf = Buffer::no_color();
            emit(
                &mut text_buf,
                &config,
                &files_of(sources),
                &detailed_user_python_message(py, err.as_ref()),
            )
            .expect("Failed to create error message");
            String::from_utf8(text_buf.into_inner()).expect("Error message was not valid UTF-8")
        }
    }
}

fn prim_label_of(span: &ParseSpan, message: impl Into<String>) -> Label<usize> {
    Label {
        style: LabelStyle::Primary,
        file_id: span.file_idx(),
        range: (span.start().byte_ofs..span.end().byte_ofs),
        message: message.into(),
    }
}

fn sec_label_of(span: &ParseSpan, message: impl Into<String>) -> Label<usize> {
    Label {
        style: LabelStyle::Secondary,
        file_id: span.file_idx(),
        range: (span.start().byte_ofs..span.end().byte_ofs),
        message: message.into(),
    }
}

fn error_diag<T>(
    message: impl Into<String>,
    labels: Vec<Label<T>>,
    notes: Vec<String>,
) -> Diagnostic<T> {
    Diagnostic {
        severity: Severity::Error,
        code: None,
        message: message.into(),
        labels,
        notes,
    }
}

fn push_inline_mode_ctx_labels(labels: &mut Vec<Label<usize>>, inl_mode: &InlineModeContext) {
    match inl_mode {
        InlineModeContext::Paragraph(c) => {
            labels.push(sec_label_of(&c.first_tok(), "Paragraph started here..."));
            labels.push(sec_label_of(&c.last_tok(), "...still in inline mode here"));
        }
        InlineModeContext::InlineScope { scope_start } => {
            labels.push(sec_label_of(scope_start, "Inline scope started here"))
        }
    }
}

/// Helper macro for making a Vec<T> where each element might have a different definition of Into<T>.
/// e.g. you could make a Vec<String> from into_vec![ "static &str", format!("Owned String") ]
macro_rules! into_vec {
    ($($x:expr,)*) => {
        vec![
            $($x.into(),)*
        ]
    };
}

fn detailed_syntax_message(py: Python, err: &TTSyntaxError) -> Diagnostic<usize> {
    use TTSyntaxError::*;
    match err {
        CodeCloseOutsideCode(span) => error_diag(
            "Code close token in text mode",
            vec![prim_label_of(span, "Code-close here")],
            into_vec!["If you want to use ']' in text, try escaping it with a backslash '\\]'",],
        ),
        BlockScopeCloseOutsideScope(span) => error_diag(
            "Scope-close token found in block-mode when no block scopes are open",
            vec![prim_label_of(span, "Scope-close here")],
            into_vec!["If you want to use '}' in text, try escaping it with a backslash '\\}'",],
        ),
        InlineScopeCloseOutsideScope(span) => error_diag(
            "Scope-close token found in inline-mode when no inline scopes are open",
            vec![prim_label_of(span, "Scope-close here")],
            into_vec![
                "If you want to use '}' in text, try escaping it with a backslash '\\}'",
                "If you meant to end an enclosing block-scope, put the '}' on a separate line.",
            ],
        ),
        RawScopeCloseOutsideRawScope(span) => error_diag(
            "Raw-scope-close token found when no raw scope is open",
            vec![prim_label_of(span, "Raw-scope-close here")],
            into_vec![
                "If you want to use this in text, try escaping each character with a backslash.",
                "e.g. '}###' = '\\}\\#\\#\\#'",
            ],
        ),
        EndedInsideCode {
            code_start,
            eof_span,
        } => error_diag(
            "File ended while parsing code",
            vec![
                prim_label_of(code_start, "Started parsing code here"),
                sec_label_of(eof_span, "File ends here"),
            ],
            into_vec![
                "Make sure to close code mode!",
                "If you meant to use '[' as text instead of starting code, try escaping it with a \
                 backslash '\\['",
            ],
        ),
        EndedInsideRawScope {
            raw_scope_start,
            eof_span,
        } => error_diag(
            "File ended while parsing raw scope",
            vec![
                prim_label_of(raw_scope_start, "Started parsing raw scope here"),
                sec_label_of(eof_span, "File ends here"),
            ],
            into_vec![
                "Make sure to close the raw scope!",
                "If you meant to use this as text instead of opening a raw scope,\ntry escaping \
                 each character with a backslash",
                "e.g. '###{' = '\\#\\#\\#\\{'",
            ],
        ),
        EndedInsideScope {
            scope_start,
            eof_span,
        } => error_diag(
            "File ended inside scope",
            vec![
                prim_label_of(scope_start, "Scope opened here"),
                sec_label_of(eof_span, "File ends here"),
            ],
            into_vec![
                "Make sure to close the scope!",
                "If you meant to use '{' in text instead of opening a scope, try escaping it with \
                 a backslash '\\{'",
            ],
        ),
        BlockScopeOpenedInInlineMode {
            inl_mode,
            block_scope_open,
        } => {
            let mut labels = vec![];
            push_inline_mode_ctx_labels(&mut labels, inl_mode);
            labels.push(prim_label_of(block_scope_open, "Block scope opened here"));
            let mut notes: Vec<String> = into_vec![
                "A scope-open followed by whitespace and a newline is a block-scope-open.",
            ];
            match inl_mode {
                InlineModeContext::Paragraph(_) => {
                    notes.push("Block scopes can't be opened inside paragraphs.".into());
                    notes.push(
                        "Try opening this scope on a new line, separated from the paragraph."
                            .into(),
                    );
                }
                InlineModeContext::InlineScope { .. } => {
                    notes.push("Block scopes can't be opened inside inline scopes.".into());
                }
            }
            error_diag("Block scope opened in inline mode", labels, notes)
        }
        CodeEmittedBlockInInlineMode {
            inl_mode,
            block,
            code_span,
        } => {
            let mut labels = vec![];
            push_inline_mode_ctx_labels(&mut labels, inl_mode);
            labels.push(prim_label_of(code_span, "Block emitted by code here"));
            let mut notes = vec![format!(
                "Emitted an object '{}', which implements `Block`",
                block
                    .bind(py)
                    .str()
                    .map_or("<stringification failed>".into(), |pystring| pystring
                        .to_string())
            )];
            match inl_mode {
                InlineModeContext::Paragraph(_) => {
                    notes.push("Blocks can't be emitted inside paragraphs.".into());
                }
                InlineModeContext::InlineScope { .. } => {
                    notes.push("Blocks can't be emitted inside inline scopes.".into());
                }
            }
            error_diag(
                "A `Block` was emitted by code in inline mode.",
                labels,
                notes,
            )
        }
        CodeEmittedHeaderInInlineMode {
            inl_mode,
            header,
            code_span,
        } => {
            let mut labels = vec![];
            push_inline_mode_ctx_labels(&mut labels, inl_mode);
            labels.push(prim_label_of(code_span, "Header emitted by code here"));
            let mut notes = vec![format!(
                "Emitted an object '{}', which implements `Header`",
                stringify_py(header.bind(py))
            )];
            match inl_mode {
                InlineModeContext::Paragraph(_) => {
                    notes.push("Headers can't be emitted inside paragraphs.".into());
                }
                InlineModeContext::InlineScope { .. } => {
                    notes.push("Headers can't be emitted inside inline scopes.".into());
                }
            }
            error_diag(
                "A `Header` was emitted by code in inline mode.",
                labels,
                notes,
            )
        }
        CodeEmittedHeaderInBlockScope {
            block_scope_start,
            header,
            // FUTURE could rework code displaying to not include the argument.
            // For example, it could include code start, code end/scope start, scope end...
            // then codespan won't need to print the whole argument which could be long.
            code_span,
        } => error_diag(
            "A `Header` was emitted by code inside a block scope.",
            vec![
                sec_label_of(block_scope_start, "Enclosing block scope started here"),
                prim_label_of(code_span, "Header emitted by code here"),
            ],
            into_vec![
                format!(
                    "Emitted an object '{}', which implements `Header`",
                    stringify_py(header.bind(py))
                ),
                "Headers are only allowed at the top level of the document,\nnot inside block \
                 scopes.",
            ],
        ),
        CodeEmittedSourceInInlineMode {
            inl_mode,
            code_span,
        } => {
            let mut labels = vec![];
            push_inline_mode_ctx_labels(&mut labels, inl_mode);
            labels.push(prim_label_of(
                code_span,
                "TurnipTextSource emitted by code here",
            ));
            let mut notes = vec![];
            match inl_mode {
                InlineModeContext::Paragraph(_) => {
                    notes.push("New source files can't be emitted inside paragraphs.".into());
                }
                InlineModeContext::InlineScope { .. } => {
                    notes.push("New source files can't be emitted inside inline scopes.".into());
                }
            }
            error_diag(
                "A `TurnipTextSource` file was emitted by code in inline mode.",
                labels,
                notes,
            )
        }
        SentenceBreakInInlineScope {
            scope_start,
            sentence_break,
        } => error_diag(
            "Paragraph break found inside an inline scope",
            vec![
                prim_label_of(scope_start, "Inline scope opened here"),
                sec_label_of(sentence_break, "Sentence break here"),
            ],
            into_vec![
                "Inline scopes can only contain one line.",
                "Try closing the inline scope with '}', escaping the sentence-break with a \
                 backslash '\\',\nor make it a block scope by putting a newline directly after \
                 the scope-open.",
            ],
        ),
        EscapedNewlineInBlockMode { newline } => error_diag(
            "Escaped newline found in block-mode",
            vec![prim_label_of(newline, "Backslash-escaped newline here")],
            into_vec![
                "Escaping a newline with a backslash means 'continue the line'.",
                "It is valid inside Paragraphs and inline scopes, but it doesn't have any meaning \
                 in block mode.",
                "Delete the backslash to remove this error.",
            ],
        ),
        InsufficientBlockSeparation {
            last_block,
            next_block_start,
        } => {
            let mut labels = vec![];
            use BlockModeElem::*;
            match last_block {
                HeaderFromCode(s) => labels.push(sec_label_of(s, "A Header was emitted here...")),
                Para(c) => {
                    labels.push(sec_label_of(&c.first_tok(), "A Paragraph started here..."));
                    labels.push(sec_label_of(
                        &c.last_tok(),
                        "...and was still in progress here...",
                    ))
                }
                BlockScope(c) => {
                    labels.push(sec_label_of(
                        &c.first_tok(),
                        "A block scope started here...",
                    ));
                    labels.push(sec_label_of(&c.last_tok(), "...and ended here..."))
                }
                BlockFromCode(s) => labels.push(sec_label_of(s, "A Block was emitted here...")),
                SourceFromCode(s) => {
                    labels.push(sec_label_of(s, "A new source file was emitted here..."))
                }
                AnyToken(s) => labels.push(sec_label_of(s, "Some Block was generated by this...")),
            };
            match next_block_start {
                HeaderFromCode(s) => labels.push(prim_label_of(
                    s,
                    "...need a new line before emitting a Header here.",
                )),
                Para(c) => {
                    labels.push(prim_label_of(
                        &c.first_tok(),
                        "...need a new line before starting a Paragraph here.",
                    ))
                    // Don't care about the end of the paragraph
                }
                BlockScope(c) => {
                    labels.push(prim_label_of(
                        &c.first_tok(),
                        "...need a new line before starting a block scope here.",
                    ))
                    // Don't care about the end of the block scope.
                }
                BlockFromCode(s) => labels.push(prim_label_of(
                    s,
                    "...need a new line before emitting a block here.",
                )),
                SourceFromCode(s) => labels.push(prim_label_of(
                    s,
                    "...need a new line before emitting a source file here.",
                )),
                AnyToken(s) => labels.push(prim_label_of(
                    s,
                    "...need a new line before any other content.",
                )),
            }
            error_diag(
                "Insufficient separation between block-level elements",
                labels,
                into_vec![
                    "Blocks must be visually separated in turnip_text code.",
                    "Start the new block on a new line.",
                    "Headers and TurnipTextSource files are also block-level elements, and must \
                     also be separated.",
                ],
            )
        }
    }
}

fn detailed_user_python_message(py: Python, err: &TTUserPythonError) -> Diagnostic<usize> {
    use TTUserPythonError::*;
    match err {
        CompilingEvalBrackets {
            code_ctx,
            code_n_hyphens,
            code,
            mode,
            err,
        } => {
            let mut notes = vec![];
            match mode {
                UserPythonCompileMode::EvalExpr => {
                    notes.push(format!(
                        "Trying to compile the following code as a Python expression raised {}.",
                        err.get_type_bound(py).to_string()
                    ));
                }
                UserPythonCompileMode::ExecStmts => {
                    notes.push(
                        "Trying to compile the following code as a Python expression raised \
                         SyntaxError, "
                            .into(),
                    );
                    notes.push(format!(
                        "then trying to compile it as at least one Python statement raised {}.",
                        err.get_type_bound(py).to_string()
                    ));
                }
                UserPythonCompileMode::ExecIndentedStmts => {
                    notes.push(
                        "Trying to compile the code as a Python statement raised IndentationError."
                            .into(),
                    );
                    notes.push(format!(
                        "Attached 'if True:' to the front to fix it, but compiling that raised {}.",
                        err.get_type_bound(py).to_string()
                    ));
                }
            }
            // Brittle test for if it's due to bad bracketing
            // e.g. the code `[  len([1, 2, 3])  ]` is going to be pythonized as `len([1, 2, 3` - the leading whitespace will be trimmed, the ']' inside the code will close the Python early.
            // This won't happen if you're using [- -], because the only way that can fail is if you have -] in python and I'm pretty sure that's not valid syntax.
            if *code_n_hyphens == 0
                && err.is_instance_of::<PySyntaxError>(py)
                && stringify_py(err.value_bound(py)).contains("'[' was never closed")
            {
                notes.push(
                    "This may be because the code is closed early by a ']' character.".into(),
                );
                notes.push(
                    "To use '[' ']' inside Python, place '-' directly before and after the eval-brackets."
                        .into(),
                );
                notes.push("e.g. '[- len([1, 2, 3]) -]'".into())
            }
            notes.push(format!(
                "Compiled code:\n{}",
                code.clone()
                    .into_string()
                    .expect("Failed to stringify code"),
            ));
            error_diag(
                "Error when compiling Python from eval-brackets",
                vec![prim_label_of(&code_ctx.full_span(), "Code taken from here")],
                notes,
            )
        }
        RunningEvalBrackets {
            code_ctx,
            code,
            mode,
            err,
        } => {
            let mut notes = vec![];
            match mode {
                UserPythonCompileMode::EvalExpr => {}
                UserPythonCompileMode::ExecStmts => {
                    notes.push(
                        "Trying to compile the following code as a Python expression raised \
                         SyntaxError."
                            .into(),
                    );
                    notes.push("Compiling it as a Python statement succeeded.".into());
                }
                UserPythonCompileMode::ExecIndentedStmts => {
                    notes.push(
                        "Trying to compile the code as Python statements raised IndentationError."
                            .into(),
                    );
                    notes.push("Attaching 'if True:' to the front made the code compile.".into());
                }
            }
            notes.push(format!(
                "Executed code:\n{}",
                code.clone()
                    .into_string()
                    .expect("Failed to stringify code"),
            ));
            error_diag(
                format!(
                    "{} raised when executing Python from eval-brackets",
                    err.get_type_bound(py).to_string()
                ),
                vec![prim_label_of(&code_ctx.full_span(), "Code taken from here")],
                notes,
            )
        }
        CoercingEvalBracketToElement { code_ctx, obj } => {
            let obj = obj.bind(py);
            let mut notes = into_vec![
                "To emit an object into the document it must be None, a TurnipTextSource, a \
                 Header, a Block, or an Inline.",
            ];
            // Print the name if it has one, always print stringification
            // Decided against trying qualname - that will usually come across in stringification
            if let Some(name) = get_name(obj) {
                notes.push(format!("Instead, Python emitted '{name}'"));
                notes.push(format!("which is '{}'", stringify_py(obj)));
            } else {
                notes.push(format!("Instead, Python emitted '{}'", stringify_py(obj)));
            }
            // Print the docstring if it has one
            if let Some(doc) = get_docstring(obj) {
                notes.push(format!("which had a docstring:\n{}", doc));
            }
            // If it's callable, suggest calling it
            if obj.is_callable() {
                notes.push("This object is callable - try calling it?".into());
            }
            // If it's a different builder, suggest building with the correct argument
            if matches!(BlockScopeBuilder::fits_typeclass(obj), Ok(true)) {
                notes.push(
                    "This object fits BlockScopeBuilder - try attaching a block scope?".into(),
                );
            }
            if matches!(InlineScopeBuilder::fits_typeclass(obj), Ok(true)) {
                notes.push(
                    "This object fits InlineScopeBuilder - try attaching an inline scope?".into(),
                );
            }
            if matches!(RawScopeBuilder::fits_typeclass(obj), Ok(true)) {
                notes.push("This object fits RawScopeBuilder - try attaching a raw scope?".into());
            }
            error_diag(
                "Python code produced an unsupported object",
                vec![prim_label_of(
                    &code_ctx.full_span(),
                    "Object produced from this code",
                )],
                notes,
            )
        }
        CoercingEvalBracketToBuilder {
            code_ctx,
            scope_open,
            build_mode,
            obj,
            err: _,
        } => {
            let obj = obj.bind(py);
            let (argument_name, builder_type) = match build_mode {
                UserPythonBuildMode::FromBlock => ("a block scope", "a BlockScopeBuilder"),
                UserPythonBuildMode::FromInline => ("an inline scope", "an InlineScopeBuilder"),
                UserPythonBuildMode::FromRaw => ("a raw scope", "a RawScopeBuilder"),
            };
            let mut notes = into_vec![format!(
                "If eval-brackets are attached to {argument_name}, the produced object \
                     must be a {builder_type}"
            ),];
            // Print the name if it has one, always print stringification
            // Decided against trying qualname - that will usually come across in stringification
            if let Some(name) = get_name(obj) {
                notes.push(format!("Instead, Python emitted '{name}'"));
                notes.push(format!("which is '{}'", stringify_py(obj)));
            } else {
                notes.push(format!("Instead, Python emitted '{}'", stringify_py(obj)));
            }
            // Print the docstring if it has one
            if let Some(doc) = get_docstring(obj) {
                notes.push(format!("which had a docstring:\n{}", doc));
            }
            // If it's a DocElement by itself, suggest removing the argument
            if matches!(Header::fits_typeclass(obj), Ok(true)) {
                notes.push("The builder does fit Header, try removing the argument".into());
            }
            if matches!(Block::fits_typeclass(obj), Ok(true)) {
                notes.push("The builder does fit Block, try removing the argument".into());
            }
            if matches!(Inline::fits_typeclass(obj), Ok(true)) {
                notes.push("The builder does fit Inline, try removing the argument".into());
            }
            if obj.is_exact_instance_of::<PyString>()
                || obj.is_exact_instance_of::<PyLong>()
                || obj.is_exact_instance_of::<PyFloat>()
            {
                notes.push("The builder is coercible to Inline, try removing the argument".into());
            }
            if obj.is_exact_instance_of::<TurnipTextSource>() {
                notes.push("The builder is a TurnipTextSource, try removing the argument".into());
            }
            // If it's callable, suggest calling it
            if obj.is_callable() {
                notes.push("This object is callable - try calling it to get a builder?".into());
            }
            // If it's a different builder, suggest building with the correct argument
            if matches!(BlockScopeBuilder::fits_typeclass(obj), Ok(true)) {
                notes.push(
                    "The builder does fit BlockScopeBuilder, try attaching a block scope instead"
                        .into(),
                );
            }
            if matches!(InlineScopeBuilder::fits_typeclass(obj), Ok(true)) {
                notes.push("The builder does fit InlineScopeBuilder, try attaching an inline scope instead".into());
            }
            if matches!(RawScopeBuilder::fits_typeclass(obj), Ok(true)) {
                notes.push(
                    "The builder does fit RawScopeBuilder, try attaching a raw scope instead"
                        .into(),
                );
            }

            error_diag(
                format!("Python code attached to {argument_name} didn't produce {builder_type}"),
                vec![
                    prim_label_of(
                        &code_ctx.full_span(),
                        format!("Object produced here wasn't {builder_type}"),
                    ),
                    sec_label_of(scope_open, "Scope attached here"),
                ],
                notes,
            )
        }
        Building {
            code_ctx,
            arg_ctx,
            build_mode,
            builder,
            err,
        } => {
            let (builder_type, builder_function) = match build_mode {
                UserPythonBuildMode::FromBlock => ("BlockScopeBuilder", ".build_from_blocks()"),
                UserPythonBuildMode::FromInline => ("InlineScopeBuilder", ".build_from_inlines()"),
                UserPythonBuildMode::FromRaw => ("RawScopeBuilder", ".build_from_raw()"),
            };
            error_diag(
                format!(
                    "{} raised when building an object from an evaluated {builder_type}",
                    err.get_type_bound(py).to_string(),
                ),
                vec![
                    prim_label_of(&code_ctx.full_span(), "Code created a builder..."),
                    sec_label_of(&arg_ctx.full_span(), "...and took this argument"),
                ],
                into_vec![
                    format!(
                        "The code successfully evaluated to the builder {}",
                        stringify_py(builder.bind(py))
                    ),
                    format!(
                        "Calling {builder_function} on this object with the scope argument raised \
                         an error"
                    ),
                ],
            )
        }
        CoercingBuildResultToElement {
            code_ctx,
            arg_ctx,
            builder,
            obj,
            err: _,
        } => error_diag(
            "Python code created a builder, and built a new object that isn't supported",
            vec![
                prim_label_of(&code_ctx.full_span(), "Code created a builder..."),
                sec_label_of(&arg_ctx.full_span(), "...and took this argument"),
            ],
            into_vec![
                format!(
                    "The code successfully evaluated to the builder {}",
                    stringify_py(builder.bind(py))
                ),
                format!(
                    "The builder took a scope argument and successfully built {},\nbut it wasn't \
                     of a supported type.",
                    stringify_py(obj.bind(py))
                ),
                "To emit an object into the document it must be None, a Header, a Block, or an \
                 Inline.",
            ],
        ),
    }
}
