use pyo3::{prelude::*, types::PyDict};

use crate::{
    error::TurnipTextContextlessResult,
    lexer::TTToken,
    util::{ParseContext, ParseSpan},
};

use super::{BuildFromTokens, BuildStatus, PushToNextLevel};

pub struct CommentProcessor {}
impl CommentProcessor {
    pub fn new() -> Self {
        Self {}
    }
}
impl BuildFromTokens for CommentProcessor {
    fn process_token(
        &mut self,
        _py: Python,
        _py_env: &PyDict,
        tok: TTToken,
        _data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        // This builder does not directly emit new source files, so it cannot receive tokens from inner files.
        // When receiving EOF it returns [BuildStatus::DoneAndReprocessToken].
        // This fulfils the contract for [BuildFromTokens::process_token].
        match tok {
            TTToken::Newline(_) | TTToken::EOF(_) => Ok(BuildStatus::DoneAndReprocessToken(None)),
            _ => Ok(BuildStatus::Continue),
        }
    }

    fn process_push_from_inner_builder(
        &mut self,
        _py: Python,
        _py_env: &PyDict,
        _pushed: Option<PushToNextLevel>,
        // closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        unreachable!("CommentFromTokens does not spawn inner builders")
    }

    fn on_emitted_source_inside(
        &mut self,
        _code_emitting_source: ParseContext,
    ) -> TurnipTextContextlessResult<()> {
        unreachable!("CommentFromTokens does not spawn an inner code builder, so cannot have a source file emitted inside")
    }
    fn on_emitted_source_closed(&mut self, _inner_source_emitted_by: ParseSpan) {
        unreachable!("CommentFromTokens does not spawn an inner code builder, so cannot have a source file emitted inside")
    }
}
