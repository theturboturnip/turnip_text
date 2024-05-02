use pyo3::prelude::*;

use crate::{
    error::TurnipTextContextlessResult,
    interpreter::ParserEnv,
    lexer::{Escapable, TTToken},
    util::{ParseContext, ParseSpan},
};

use super::{EmittedElement, ProcStatus, TokenProcessor};

pub struct CommentProcessor {}
impl CommentProcessor {
    pub fn new() -> Self {
        Self {}
    }
}
impl TokenProcessor for CommentProcessor {
    fn process_token(
        &mut self,
        _py: Python,
        _py_env: ParserEnv,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        // This builder does not directly emit new source files, so it cannot receive tokens from inner files.
        // When receiving EOF it returns [ProcStatus::PopAndReprocessToken].
        // This fulfils the contract for [TokenProcessor::process_token].
        match tok {
            // Escaped(Newline) ends the comment.
            // Python does this too, presumably because escaping a newline has semantic purpose (to keep content in the same expression) and it is nice to be able to add documentation comments to each "not-line" without nuking the following content.
            // TODO test that escaped newlines are reprocessed correctly in block and inline modes. They should continue lines in paragraphs, be ignored in inline scopes, and throw errors in block modes
            TTToken::Escaped(_, Escapable::Newline) | TTToken::Newline(_) | TTToken::EOF(_) => {
                Ok(ProcStatus::PopAndReprocessToken(None))
            }
            _ => Ok(ProcStatus::Continue),
        }
    }

    fn process_emitted_element(
        &mut self,
        _py: Python,
        _py_env: ParserEnv,
        _pushed: Option<EmittedElement>,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        unreachable!("CommentProcessor does not spawn inner builders")
    }

    fn on_emitted_source_inside(
        &mut self,
        _code_emitting_source: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        unreachable!(
            "CommentProcessor does not spawn an inner code builder, so cannot have a source file \
             emitted inside"
        )
    }
    fn on_emitted_source_closed(&mut self, _inner_source_emitted_by: ParseSpan) {
        unreachable!(
            "CommentProcessor does not spawn an inner code builder, so cannot have a source file \
             emitted inside"
        )
    }
}
