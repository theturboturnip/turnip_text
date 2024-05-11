import re
from contextlib import contextmanager
from typing import (
    Any,
    Callable,
    Generic,
    Iterable,
    Iterator,
    List,
    Optional,
    Self,
    Sequence,
    Type,
    TypeVar,
    cast,
)

from turnip_text import (
    Block,
    BlockScope,
    DocSegment,
    Document,
    Header,
    Inline,
    InlineScope,
    Paragraph,
    Raw,
    Text,
)
from turnip_text.env_plugins import FmtEnv, THeader
from turnip_text.render import Renderer
from turnip_text.render.dyn_dispatch import DynDispatch

from . import pandoc_types as pan

T = TypeVar("T")
TBlock = TypeVar("TBlock", bound=Block)
TInline = TypeVar("TInline", bound=Inline)
TPandocRenderer_contra = TypeVar(
    "TPandocRenderer_contra", bound="PandocRenderer", contravariant=True
)


def null_attr() -> pan.Attr:
    return ("", [], [])


def generic_join(ts: Sequence[T], joiner: T) -> List[T]:
    first = True
    items = []
    for t in ts:
        if not first:
            items.append(joiner)
        items.append(t)
        first = False
    return items


class PandocRenderer(Renderer):
    """An implementation of Renderer that builds a `pandoc.Document` which can then be processed into arbitrary output formats."""

    dispatch: "PandocDispatch[Self]"

    @classmethod
    def default_dispatch(
        cls: Type[TPandocRenderer_contra],
    ) -> "PandocDispatch[TPandocRenderer_contra]":
        """
        This is a convenience method that generates the most basic EmitterDispatch for a PandocRenderer.
        It is meant to be called by RenderSetup classes.
        It can be overridden in renderers that provide more than the basic emitters.
        """
        dispatch: PandocDispatch[TPandocRenderer_contra] = PandocDispatch()
        dispatch.register_block(BlockScope, lambda bs, r, fmt: r.make_block_scope(bs))
        dispatch.register_block(Paragraph, lambda p, r, fmt: r.make_paragraph(p))
        dispatch.register_inline(
            InlineScope, lambda inls, r, fmt: r.make_inline_scope(inls)
        )
        dispatch.register_inline(Text, lambda t, r, fmt: r.make_text(t))
        dispatch.register_inline(Raw, lambda raw, r, fmt: r.make_raw(raw))
        return dispatch

    def make_document(self: Self, doc: Document) -> pan.Pandoc:
        meta = pan.Meta({})
        # TODO collect metadata from plugins
        blocks = [self.make_block(b) for b in doc.contents]
        for seg in doc.segments:
            self.make_docsegment(seg, blocks)
        return pan.Pandoc(meta, blocks)

    def make_docsegment(
        self: Self, docsegment: DocSegment, blocks: List[pan.Block]
    ) -> None:
        blocks.append(self.make_header(docsegment.header))
        blocks.extend(self.make_block(b) for b in docsegment.contents)
        for subseg in docsegment.subsegments:
            self.make_docsegment(subseg, blocks)

    def make_header(self: Self, obj: Any) -> pan.Header:
        return self.dispatch.make_pan_header(obj, self, self.fmt)

    def make_block(self: Self, obj: Any) -> pan.Block:
        return self.dispatch.make_pan_block(obj, self, self.fmt)

    def make_inline(self: Self, obj: Any) -> pan.Inline:
        return self.dispatch.make_pan_inline(obj, self, self.fmt)

    def make_block_scope(self: Self, bs: Iterable[Block]) -> pan.Div:
        return pan.Div(null_attr(), [self.make_block(b) for b in bs])

    def make_paragraph(self: Self, p: Paragraph) -> pan.Para:
        inls: List[pan.Inline] = []
        for sentence in p:
            inls.extend(self.make_inline(inl) for inl in sentence)
            inls.append(pan.SoftBreak())
        return pan.Para(inls)

    def make_inline_scope(self, inls: Iterable[Inline]) -> pan.Span:
        return pan.Span(null_attr(), [self.make_inline(inl) for inl in inls])

    def make_text(self, text: Text) -> pan.Inline:
        # Unsure how to handle non-breaking space, so passthrough the unicode character directly for now.
        # However, breaking space is counted as inter-word space.
        words = [pan.Str(word) for word in re.split("\s+", text.text)]
        if len(words) == 1:
            return words[0]
        else:
            return pan.Span(null_attr(), generic_join(words, pan.Space()))

    def make_raw(self, raw: Raw) -> pan.Inline:
        raise TypeError("Cannot emit Raw into pandoc! It doesn't make sense!")


class PandocDispatch(Generic[TPandocRenderer_contra]):
    header_makers: DynDispatch[["TPandocRenderer_contra", FmtEnv], pan.Header]
    block_makers: DynDispatch[["TPandocRenderer_contra", FmtEnv], pan.Block]
    inline_makers: DynDispatch[["TPandocRenderer_contra", FmtEnv], pan.Inline]

    def __init__(self) -> None:
        super().__init__()
        self.header_makers = DynDispatch()
        self.block_makers = DynDispatch()
        self.inline_makers = DynDispatch()

    def register_header(
        self,
        h: Type[THeader],
        f: Callable[[THeader, TPandocRenderer_contra, FmtEnv], pan.Header],
    ) -> None:
        self.header_makers.register_handler(h, f)

    def register_block(
        self,
        h: Type[TBlock],
        f: Callable[[TBlock, TPandocRenderer_contra, FmtEnv], pan.Block],
    ) -> None:
        self.block_makers.register_handler(h, f)

    def register_inline(
        self,
        h: Type[TInline],
        f: Callable[[TInline, TPandocRenderer_contra, FmtEnv], pan.Inline],
    ) -> None:
        self.inline_makers.register_handler(h, f)

    def make_pan_header(
        self, obj: Any, renderer: TPandocRenderer_contra, fmt: FmtEnv
    ) -> pan.Header:
        f = self.header_makers.get_handler(obj)
        if not f:
            is_inline = self.inline_makers.get_handler(obj) is not None
            is_block = self.block_makers.get_handler(obj) is not None
            raise ValueError(
                f"Object {obj} didn't have a header maker. (inline maker? {is_inline}) (block maker? {is_block})"
            )
        return f(obj, renderer, fmt)

    def make_pan_block(
        self, obj: Any, renderer: TPandocRenderer_contra, fmt: FmtEnv
    ) -> pan.Block:
        f = self.block_makers.get_handler(obj)
        if not f:
            is_header = self.header_makers.get_handler(obj) is not None
            is_inline = self.inline_makers.get_handler(obj) is not None
            raise ValueError(
                f"Object {obj} didn't have a block maker. (header maker? {is_header}) (inline maker? {is_inline})"
            )
        return f(obj, renderer, fmt)

    def make_pan_inline(
        self, obj: Any, renderer: TPandocRenderer_contra, fmt: FmtEnv
    ) -> pan.Inline:
        f = self.inline_makers.get_handler(obj)
        if not f:
            is_header = self.header_makers.get_handler(obj) is not None
            is_block = self.block_makers.get_handler(obj) is not None
            raise ValueError(
                f"Object {obj} didn't have an inline maker. (header maker? {is_header}) (block maker? {is_block})"
            )
        return f(obj, renderer, fmt)
