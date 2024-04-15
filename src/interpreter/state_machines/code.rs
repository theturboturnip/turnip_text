use std::{cell::RefCell, rc::Rc};

use pyo3::{prelude::*, types::PyDict};

use crate::{
    error::TurnipTextContextlessResult,
    interpreter::{InterpError, MapContextlessResult},
    lexer::TTToken,
    python::{
        eval_or_exec,
        interop::{
            coerce_to_inline_pytcref, BlockScopeBuilder, BuilderOutcome, InlineScopeBuilder,
            RawScopeBuilder, TurnipTextSource,
        },
        typeclass::PyTcRef,
    },
    util::ParseSpan,
};

use super::{
    block::BlockOrInlineScopeFromTokens, inline::RawStringFromTokens, rc_refcell, BuildFromTokens,
    BuildStatus, BuilderContext, DocElement, PushToNextLevel,
};

pub struct CodeFromTokens {
    ctx: BuilderContext,
    n_closing: usize,
    code: String,
    evaled_code: Option<PyObject>,
}
impl CodeFromTokens {
    pub fn new(start_span: ParseSpan, n_opening: usize) -> Rc<RefCell<Self>> {
        rc_refcell(Self {
            ctx: BuilderContext::new("Code", start_span, start_span),
            n_closing: n_opening,
            code: String::new(),
            evaled_code: None,
        })
    }
}
impl BuildFromTokens for CodeFromTokens {
    fn process_token(
        &mut self,
        py: Python,
        py_env: &PyDict,
        tok: TTToken,
        data: &str,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        match &self.evaled_code {
            // If None, we're still parsing the code itself.
            None => {
                assert!(
                    self.ctx.try_extend(&tok.token_span()),
                    "Code got a token from a different file that it was opened in"
                );
                match tok {
                    TTToken::CodeClose(_, n_close_brackets)
                        if n_close_brackets == self.n_closing =>
                    {
                        let res: &PyAny = eval_or_exec(py, py_env, &self.code).err_as_interp(
                            py,
                            "Error evaluating contents of eval-brackets",
                            self.ctx.full_span(),
                        )?;

                        // If we evaluated a TurnipTextSource, it may not be a builder of any kind thus we can finish immediately.
                        if let Ok(inserted_file) = res.extract::<TurnipTextSource>() {
                            Ok(BuildStatus::DoneAndNewSource(self.ctx, inserted_file))
                        } else {
                            self.evaled_code = Some(res.into_py(py));
                            Ok(BuildStatus::Continue)
                        }
                    }
                    TTToken::EOF(_) => Err(InterpError::EndedInsideCode {
                        code_start: self.ctx.full_span(), // TODO use first_tok here
                    }
                    .into()),
                    _ => {
                        // Code blocks use raw stringification to avoid confusion between text written and text entered
                        self.code.push_str(tok.stringify_raw(data));
                        Ok(BuildStatus::Continue)
                    }
                }
            }
            // Parse one token after the code ends to see what we should do.
            Some(evaled_result) => match tok {
                TTToken::ScopeOpen(start_span) => Ok(BuildStatus::StartInnerBuilder(
                    BlockOrInlineScopeFromTokens::new(start_span),
                )),
                TTToken::RawScopeOpen(start_span, n_opening) => Ok(BuildStatus::StartInnerBuilder(
                    RawStringFromTokens::new(start_span, n_opening),
                )),

                _ => {
                    // We didn't encounter any scope openers, so we know we don't need to build anything.
                    // Emit the object directly, and reprocess the current token so it gets included.

                    // Consider: we may have an object at the very start of the line.
                    // If it's an Inline, e.g. "[virtio] is a thing..." then we want to return Inline so the rest of the line can be added.
                    // If it's a Block, e.g. [image_figure(...)], then we want to return Block.
                    // If it's neither, it needs to be *coerced*.
                    // But what should coercion look like? What should we try to coerce the object *to*?
                    // Well, what can be coerced?
                    // Coercible to inline:
                    // - Inline        -> `x`
                    // - List[Inline]  -> `InlineScope(x)`
                    // - str/float/int -> `Text(str(x))`
                    // Coercible to block:
                    // - Block             -> `x`
                    // - List[Block]       -> `BlockScope(x)`
                    // - Sentence          -> `Paragraph([x])
                    // - CoercibleToInline -> `Paragraph([Sentence([coerce_to_inline(x)])])`
                    // I do not see the need to allow eval-brackets to directly return List[Block] or Sentence at all.
                    // Similar outcomes can be acheived by wrapping in BlockScope or Paragraph manually in the evaluated code, which better demonstrates intent.
                    // If we always coerce to inline, then the wrapping in Paragraph and Sentence happens naturally in the interpreter.
                    // => We check if it's a block, and if it isn't we try to coerce to inline.

                    let evaled_result_ref = evaled_result.as_ref(py);

                    if evaled_result_ref.is_none() {
                        Ok(BuildStatus::DoneAndReprocessToken(None))
                    } else if let Ok(header) = PyTcRef::of(evaled_result_ref) {
                        Ok(BuildStatus::DoneAndReprocessToken(Some(
                            self.ctx.make(DocElement::Header(header)),
                        )))
                    } else if let Ok(block) = PyTcRef::of(evaled_result_ref) {
                        Ok(BuildStatus::DoneAndReprocessToken(Some(
                            self.ctx.make(DocElement::Block(block)),
                        )))
                    } else {
                        let inline = coerce_to_inline_pytcref(py, evaled_result_ref)
                            .err_as_interp(
                                py,
                                "This eval-bracket had no attached scope and returned something that wasn't None, Header, Block, or coercible to Inline.",
                                self.ctx.full_span(),
                            )?;
                        Ok(BuildStatus::DoneAndReprocessToken(Some(
                            self.ctx.make(DocElement::Inline(inline)),
                        )))
                    }
                }
            },
        }
    }

    fn process_push_from_inner_builder(
        &mut self,
        py: Python,
        py_env: &PyDict,
        pushed: Option<PushToNextLevel>,
    ) -> TurnipTextContextlessResult<BuildStatus> {
        let evaled_result_ref = self.evaled_code.take().unwrap().into_ref(py);

        let pushed = pushed.expect("Should never get a built None - CodeFromTokens only spawns BlockScopeFromTokens, InlineScopeFromTokens, RawScopeFromTokens none of which return None.");
        let built = match pushed.elem {
            DocElement::Block(blocks) => {
                let builder: PyTcRef<BlockScopeBuilder> =
                    PyTcRef::of_friendly(evaled_result_ref, "value returned by eval-bracket")
                    .err_as_interp(
                        py,
                        "Expected a BlockScopeBuilder because the eval-brackets were followed by a block scope", self.ctx.full_span()
                    )?;

                // Now that we know coersion is a success, update the code span
                assert!(
                    self.ctx.try_combine(pushed.from_builder),
                    "Code got a built object from a different file that it was opened in"
                );

                BlockScopeBuilder::call_build_from_blocks(py, builder, blocks.as_ref(py))
                    .err_as_internal(py)?
            }
            DocElement::Inline(inlines) => {
                let builder: PyTcRef<InlineScopeBuilder> =
                    PyTcRef::of_friendly(evaled_result_ref, "value returned by eval-bracket")
                    .err_as_interp(
                        py,
                        "Expected an InlineScopeBuilder because the eval-brackets were followed by an inline scope",
                        self.ctx.full_span()
                    )?;

                // Now that we know coersion is a success, update the code span
                assert!(
                    self.ctx.try_combine(pushed.from_builder),
                    "Code got a built object from a different file that it was opened in"
                );

                InlineScopeBuilder::call_build_from_inlines(py, builder, inlines.as_ref(py))
                    .err_as_internal(py)?
            }
            DocElement::Raw(raw) => {
                let builder: PyTcRef<RawScopeBuilder> =
                    PyTcRef::of_friendly(evaled_result_ref, "value returned by eval-bracket")
                    .err_as_interp(
                        py,
                        "Expected a RawScopeBuilder because the eval-brackets were followed by a raw scope",
                    self.ctx.full_span()
                    )?;

                // Now that we know coersion is a success, update the code span
                assert!(
                    self.ctx.try_combine(pushed.from_builder),
                    "Code got a built object from a different file that it was opened in"
                );

                RawScopeBuilder::call_build_from_raw(py, &builder, &raw).err_as_internal(py)?
            }
            _ => unreachable!("Invalid combination of requested and actual built element types"),
        };
        match built {
            BuilderOutcome::Block(block) => Ok(BuildStatus::Done(Some(
                self.ctx.make(DocElement::Block(block)),
            ))),
            BuilderOutcome::Inline(inline) => Ok(BuildStatus::Done(Some(
                self.ctx.make(DocElement::Inline(inline)),
            ))),
            BuilderOutcome::Header(header) => Ok(BuildStatus::Done(Some(
                self.ctx.make(DocElement::Header(header)),
            ))),
            BuilderOutcome::None => Ok(BuildStatus::Done(None)),
        }
    }
}
