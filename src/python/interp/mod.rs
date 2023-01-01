#![allow(dead_code)]

use anyhow::bail;
use thiserror::Error;
use pyo3::prelude::*;

use crate::{lexer::TTToken, parser::ParseSpan};
use self::para::{InterpParaState, InterpParaAction};

use super::{interop::*, TurnipTextPython};

mod para;

pub struct InterpState<'interp, 'a: 'interp> {
    /// FSM state
    block_state: InterpBlockState,
    /// Overrides InterpBlockState and raw_state - if Some(state), we are in "comment mode" and all other state machines are paused
    comment_state: Option<InterpCommentState>,
    /// Stack of block scopes
    block_stack: Vec<InterpBlockScopeState>,
    /// Root of the document
    root: Py<BlockNode>,
    /// Python interpreter and context
    ttpython: &'a mut TurnipTextPython<'interp>,
    /// Raw text data
    data: &'a str,
}

/// Block-level state for the interpreter
#[derive(Debug)]
enum InterpBlockState {
    /// Waiting for new content to transition into [Self::WritingPara] or [Self::BuildingBlockLevelCode]
    ReadyForNewBlock,
    /// Building a paragraph block node, which will be added to the parent block node once complete.
    ///
    /// Transitions to [Self::ReadyForNewBlock] after finishing the paragraph
    WritingPara(InterpParaState),
    /// Building Python-code to evaluate at the block level, outside a paragraph
    ///
    /// Transitions to [Self::AttachingBlockLevelCode] once finished
    BuildingBlockLevelCode {
        code: String,
        code_start: ParseSpan,
        expected_n_hashes: usize,
    },
    /// Having constructed some code which evaluated to a BlockScopeOwner,
    /// assert the next token is a block scope and glue this code to it.
    ///
    /// Transitions to [Self::ReadyForNewBlock]
    AttachingBlockLevelCode {
        owner: Py<BlockScopeOwner>,
        code_span: ParseSpan,
    },
}


#[derive(Debug)]
struct InterpCommentState {
    comment_start: ParseSpan,
}

#[derive(Debug)]
struct InterpBlockScopeState {
    node: Py<BlockNode>,
    scope_start: ParseSpan,
    expected_n_hashes: usize,
}



#[derive(Debug)]
pub(crate) enum InterpBlockAction {
    /// On encountering a CodeOpen at the start of a line, start gathering block level code
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::BuildingBlockLevelCode]
    /// - All others invalid
    StartBlockLevelCode(ParseSpan, usize),

    /// Having finished a block-level code close which evals to [BlockScopeOwner],
    /// start a one-token wait for an inline scope to attach it to
    ///
    /// - [InterpBlockState::BuildingBlockLevelCode] -> [InterpBlockState::AttachingBlockLevelCode]
    WaitToAttachBlockCode(Py<BlockScopeOwner>, ParseSpan),

    /// Start a paragraph, optionally executing an action on the paragraph-level state machine
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::WritingPara]
    /// - [InterpBlockState::BuildingBlockLevelCode] -> [InterpBlockState::WritingPara]
    StartParagraph(Option<InterpParaAction>),

    /// On encountering a paragraph break (a blank line), add the paragraph to the document
    ///
    /// - [InterpBlockState::WritingPara] -> [InterpBlockState::ReadyForNewBlock]
    EndParagraph,

    /// On encountering block content (i.e. a BlockScopeOpen optionally preceded by a Python scope owner),
    /// push a block scope onto the current stack
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::ReadyForNewBlock]
    /// - [InterpBlockState::AttachingBlockLevelCode] -> [InterpBlockState::ReadyForNewBlock]
    PushBlock(Option<Py<BlockScopeOwner>>, ParseSpan, usize),

    /// On encountering a scope close, pop the current block from the scope
    ///
    /// - [InterpBlockState::ReadyForNewBlock] -> [InterpBlockState::ReadyForNewBlock]
    PopBlock(ParseSpan),
}

#[derive(Debug)]
pub(crate) enum InterpSpecialAction {
    /// On encountering a comment starter, go into comment mode
    StartComment(ParseSpan),

    /// Leave comment mode
    EndComment,
}



#[derive(Debug)]
pub(crate) enum InlineNodeToCreate {
    UnescapedText(String),
    RawText(String),
}

/// Enumeration of all possible interpreter errors
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InterpError {
    #[error("Code close encountered outside of code mode")]
    CodeCloseOutsideCode(ParseSpan),
    #[error("Scope close encountered with no matching scope open")]
    ScopeCloseOutsideScope(ParseSpan),
    #[error("Scope close with {n_hashes} hashes encountered when closest scope open has {expected_closing_hashes}")]
    MismatchingScopeClose {
        n_hashes: usize,
        expected_closing_hashes: usize,
        scope_open_span: ParseSpan,
        scope_close_span: ParseSpan,
    },
    #[error("File ended inside code block")]
    EndedInsideCode { code_start: ParseSpan },
    #[error("File ended inside raw scope")]
    EndedInsideRawScope { raw_scope_start: ParseSpan },
    #[error("File ended inside scope")]
    EndedInsideScope { scope_start: ParseSpan },
    #[error("Block scope encountered mid-line")]
    BlockScopeOpenedMidPara { scope_start: ParseSpan },
    #[error("Inline scope contained paragraph break")]
    ParaBreakInInlineScope {
        scope_start: ParseSpan,
        para_break: ParseSpan,
    },
    // #[error("Python error: {pyerr}")]
    // PythonErr { pyerr: Py<PyErr>, code_span: ParseSpan }
    #[error("Block scope owner was not followed by a block scope")]
    BlockOwnerCodeHasNoScope { code_span: ParseSpan },
    #[error("Inline scope owner was not followed by an inline scope")]
    InlineOwnerCodeHasNoScope { code_span: ParseSpan },
}

impl<'interp, 'a> InterpState<'interp, 'a> {
    fn new(ttpython: &'a mut TurnipTextPython<'interp>, data: &'a str) -> PyResult<Self> {
        let root: Py<BlockNode> = ttpython.with_gil(|py, _| Py::new(py, BlockNode::new()))?;
        Ok(Self {
            block_state: InterpBlockState::ReadyForNewBlock,
            comment_state: None,
            block_stack: vec![],
            root,
            ttpython,
            data,
        })
    }

    pub fn handle_token(&mut self, tok: TTToken) -> anyhow::Result<()> {
        let actions = self.compute_action(tok)?;
        self.handle_action(actions)
    }

    /// Return (block action, special action) to be executed in the order (block action, special action)
    fn compute_action(
        &mut self,
        tok: TTToken,
    ) -> anyhow::Result<(Option<InterpBlockAction>, Option<InterpSpecialAction>)> {
        use InterpBlockAction::*;
        use TTToken::*;

        // Handle comments separately
        if let Some(InterpCommentState { comment_start: _ }) = self.comment_state {
            let action = match tok {
                Newline(_) => Some(InterpSpecialAction::EndComment),
                _ => None,
            };
            // No change at the block level, potentially exit comment as a special action
            return Ok((None, action));
        }

        let action = match &mut self.block_state {
            InterpBlockState::ReadyForNewBlock => {
                match tok {
                    CodeOpen(span, n_hashes) => (Some(StartBlockLevelCode(span, n_hashes)), None),

                    // PushBlock with no code managing it
                    BlockScopeOpen(span, n_hashes) => (Some(PushBlock(None, span, n_hashes)), None),

                    // PushInlineScope with no code managing it
                    InlineScopeOpen(span, n_hashes) => (
                        Some(StartParagraph(Some(InterpParaAction::PushInlineScope(
                            None, span, n_hashes,
                        )))),
                        None,
                    ),

                    // StartRawBlock
                    RawScopeOpen(span, n_hashes) => (
                        Some(StartParagraph(Some(InterpParaAction::StartRawScope(
                            span, n_hashes,
                        )))),
                        None,
                    ),

                    // Try a scope close
                    ScopeClose(_, _) => todo!(),

                    // Complain - not in code mode
                    CodeClose(span, _) => bail!(InterpError::CodeCloseOutsideCode(span)),

                    // Do nothing - we're still ready to receive a new block
                    Newline(_) => (None, None),

                    // Enter comment mode
                    Hashes(span, _) => (None, Some(InterpSpecialAction::StartComment(span))),

                    // Normal text - start a new paragraph
                    _ => (
                        Some(StartParagraph(Some(InterpParaAction::PushInlineContent(
                            InlineNodeToCreate::UnescapedText(tok.stringify(self.data).into()),
                        )))),
                        None,
                    ),
                }
            }
            InterpBlockState::WritingPara(state) => state.handle_token(tok, self.data)?,
            InterpBlockState::BuildingBlockLevelCode {
                code,
                code_start,
                expected_n_hashes,
            } => {
                let code_span = compute_action_for_code_mode(
                    self.data,
                    tok,
                    code,
                    code_start,
                    *expected_n_hashes,
                );
                match code_span {
                    Some(_code_span) => {
                        // The code ended...
                        let _eval_result = todo!("eval code and see if it's a block owner");
                        // if eval_result is a BlockScopeOwner, WaitToAttachBlockCode
                        // elif eval_result is an InlineScopeOwner, WaitToAttachInlineCode
                        // else stringify and PushInlineContent
                    }
                    None => (None, None),
                }
            }
            InterpBlockState::AttachingBlockLevelCode { owner, code_span } => match tok {
                BlockScopeOpen(span, n_hashes) => {
                    (Some(PushBlock(Some(owner.clone()), span, n_hashes)), None)
                }
                _ => bail!(InterpError::BlockOwnerCodeHasNoScope {
                    code_span: *code_span
                }),
            },
        };

        Ok(action)
    }

    /// May recurse if StartParagraph(action)
    fn handle_action(
        &mut self,
        actions: (Option<InterpBlockAction>, Option<InterpSpecialAction>),
    ) -> anyhow::Result<()> {
        let (block_action, special_action) = actions;

        if let Some(action) = block_action {
            use InterpBlockAction as A;
            use InterpBlockState as S;
            let new_block_state = match (&self.block_state, action) {
                (S::ReadyForNewBlock, A::StartBlockLevelCode(code_start, expected_n_hashes)) => {
                    S::BuildingBlockLevelCode {
                        code: "".into(),
                        code_start,
                        expected_n_hashes,
                    }
                }
                (S::BuildingBlockLevelCode { .. }, A::WaitToAttachBlockCode(owner, code_span)) => {
                    S::AttachingBlockLevelCode { owner, code_span }
                }

                (
                    S::ReadyForNewBlock | S::BuildingBlockLevelCode { .. },
                    A::StartParagraph(action),
                ) => {
                    let para_state: InterpParaState =
                        todo!("New InterpParaState with LineStart as inl_state");
                    let requested_action = para_state.handle_action(action, self.data);
                    // TODO this should really only handle SpecialAction
                    self.handle_action(actions);
                    S::WritingPara(para_state)
                }
                (S::WritingPara(para_state), A::EndParagraph) => {
                    todo!("Push para_state.para onto the topmost entry in the block stack or the root");
                    S::ReadyForNewBlock
                }

                (
                    S::ReadyForNewBlock | S::AttachingBlockLevelCode { .. },
                    A::PushBlock(owner, scope_start, expected_n_hashes),
                ) => {
                    self.block_stack.push(InterpBlockScopeState {
                        node: todo!("Create python block node from owner"),
                        scope_start,
                        expected_n_hashes,
                    });
                    S::ReadyForNewBlock
                }
                (S::ReadyForNewBlock, A::PopBlock(scope_close_span)) => {
                    let popped_scope = self.block_stack.pop();
                    match popped_scope {
                        Some(popped_scope) => {
                            todo!("Insert popped_scope at the new topmost scope or in the document")
                        }
                        // TODO specify *block* scope
                        None => bail!(InterpError::ScopeCloseOutsideScope(scope_close_span)),
                    }
                }
                (_, action) => bail!(
                    "Invalid block state/action pair encountered ({0:?}, {1:?})",
                    self.block_state,
                    action
                ),
            };
            self.block_state = new_block_state;
        }

        if let Some(action) = special_action {
            match (&self.comment_state, action) {
                (Some(_), InterpSpecialAction::EndComment) => {
                    self.comment_state = None;
                }
                (None, InterpSpecialAction::StartComment(comment_start)) => {
                    self.comment_state = Some(InterpCommentState { comment_start })
                }
                (_, action) => bail!(
                    "Invalid special state/action pair encountered ({0:?}, {1:?})",
                    self.comment_state,
                    action
                ),
            }
        }

        Ok(())
    }
}

/// Returns Some(code_span) once the code has been closed
fn compute_action_for_code_mode(
    data: &str,
    tok: TTToken,
    code: &mut String,
    code_start: &ParseSpan,
    expected_n_hashes: usize,
) -> Option<ParseSpan> {
    match tok {
        TTToken::CodeClose(close_span, n) if n == expected_n_hashes => Some(ParseSpan {
            start: code_start.start,
            end: close_span.end,
        }),
        _ => {
            code.push_str(tok.stringify(data));
            None
        }
    }
}
