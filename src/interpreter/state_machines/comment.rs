use std::{cell::RefCell, rc::Rc};

use pyo3::{prelude::*, types::PyDict};

use crate::{error::TurnipTextContextlessResult, lexer::TTToken};

use super::{rc_refcell, BuildFromTokens, BuildStatus, PushToNextLevel};

pub struct CommentFromTokens {}
impl CommentFromTokens {
    pub fn new() -> Rc<RefCell<Self>> {
        rc_refcell(Self {})
    }
}
impl BuildFromTokens for CommentFromTokens {
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
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
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
        // closing_token: TTToken,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        panic!("CommentFromTokens does not spawn inner builders")
    }
}
