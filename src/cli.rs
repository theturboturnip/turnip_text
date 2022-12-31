use annotate_snippets::{display_list::DisplayList, snippet::*};
use anyhow::bail;
use lexer_rs::{Lexer, LexerOfStr, PosnInCharStream};

use crate::{
    lexer::{LexError, LexPosn, LexToken, Unit, units_to_tokens},
    parser::{parse_simple_tokens, ParseToken, ParseError, ParseSpan},
};

pub trait GivesCliFeedback {
    fn get_snippet<'a>(&self, file_src: &'a str) -> Snippet<'a>;
}
impl GivesCliFeedback for LexError {
    fn get_snippet<'a>(&self, file_src: &'a str) -> Snippet<'a> {
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
                    // TODO: will break on non-ASCII/non-single-byte
                    range: (self.pos.byte_ofs(), self.pos.byte_ofs() + 1),
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
        range: (span.start.byte_ofs, span.end.byte_ofs),
        label,
        annotation_type,
    }
}

impl GivesCliFeedback for ParseError {
    fn get_snippet<'a>(&self, file_src: &'a str) -> Snippet<'a> {
        use ParseError::*;
        match self {
            CodeCloseInText(span) => snippet_from_parse_span(
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
            MismatchingScopeClose {
                n_hashes: _,
                expected_closing_hashes: _,
                scope_open_span,
                scope_close_span,
            } => Snippet {
                title: Some(Annotation {
                    label: Some("Scope close with mismatching hash length"),
                    id: None,
                    annotation_type: AnnotationType::Error,
                }),
                footer: vec![Annotation {
                    label: Some("If you intended to close the scope, make the number of hashes match.\nOtherwise, try backslash-escaping the squiggly ending brace."),
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
                            scope_open_span,
                        ),
                        annotation_from_parse_span(
                            "Scope close here",
                            AnnotationType::Error,
                            scope_close_span,
                        ),
                    ],
                }],
                opt: Default::default(),
            },
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
        }
    }
}

fn display_cli_feedback<T: GivesCliFeedback>(data: &str, err: &T) {
    let dl = DisplayList::from(err.get_snippet(&data));
    eprintln!("{}", dl);
}
pub fn parse_file(path: &std::path::Path) -> anyhow::Result<Vec<ParseToken>> {
    let data = std::fs::read_to_string(path)?;

    let mut units = vec![];
    let lexer = LexerOfStr::<LexPosn, LexToken, LexError>::new(&data);

    for u in lexer.iter(&[
        Box::new(Unit::parse_special),
        Box::new(Unit::parse_other),
    ]) {
        units.push(u.map_err(|err| {
            display_cli_feedback(&data, &err);
            err
        })?);
    }

    let tokens = units_to_tokens(units);

    match parse_simple_tokens(&data, Box::new(tokens.into_iter())) {
        Ok(tokens) => Ok(tokens),
        Err(err) => {
            display_cli_feedback(&data, &err);
            bail!(err)
        }
    }
}
