use pyo3::{prelude::*, types::PyDict};

use crate::{
    error::TurnipTextContextlessResult,
    lexer::TTToken,
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
        _py_env: &PyDict,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<ProcStatus> {
        // This builder does not directly emit new source files, so it cannot receive tokens from inner files.
        // When receiving EOF it returns [ProcStatus::PopAndReprocessToken].
        // This fulfils the contract for [TokenProcessor::process_token].
        match tok {
            // TODO decide if escaped(newline) ends the comment too. By Python rules, it doesn't.
            TTToken::Newline(_) | TTToken::EOF(_) => Ok(ProcStatus::PopAndReprocessToken(None)),
            _ => Ok(ProcStatus::Continue),
        }
    }

    fn process_emitted_element(
        &mut self,
        _py: Python,
        _py_env: &PyDict,
        _pushed: Option<EmittedElement>,
        // closing_token: TTToken,
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
