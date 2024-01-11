import abc
import io
import os
from contextlib import contextmanager
from typing import (
    Any,
    Callable,
    Concatenate,
    Dict,
    Generator,
    Generic,
    Iterable,
    Iterator,
    List,
    Optional,
    ParamSpec,
    Protocol,
    Self,
    Sequence,
    Set,
    Tuple,
    Type,
    TypeVar,
    Union,
)

from turnip_text import (
    Block,
    BlockScope,
    DocSegment,
    DocSegmentHeader,
    Inline,
    InlineScope,
    Paragraph,
    Sentence,
    UnescapedText,
)
from turnip_text.doc import DocState, Document, FormatContext
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.render.counters import CounterChainValue, CounterSet, DocCounter

T = TypeVar("T")
P = ParamSpec("P")
TReturn = TypeVar("TReturn")
TBlockOrInline = TypeVar("TBlockOrInline", bound=Union[Block, Inline])
THeader = TypeVar("THeader", bound=DocSegmentHeader)
TVisitable = TypeVar("TVisitable", bound=Union[Block, Inline, DocSegmentHeader])


class DynamicNodeDispatch(Generic[P, TReturn]):
    _table: Dict[Type[Any], Callable[Concatenate[Any, P], TReturn]]

    def __init__(self) -> None:
        super().__init__()
        self._table = {}

    def register_handler(
        self,
        t: Type[T],
        f: Callable[Concatenate[T, P], TReturn],
    ) -> None:
        if t in self._table:
            raise RuntimeError(f"Conflict: registered two handlers for {t}")
        # We know that we only assign _table[t] if f takes t, and that when we pull it
        # out we will always call f with something of type t.
        # mypy doesn't know that, so we say _table stores functions taking T (the base class)
        # and sweep the difference under the rug
        self._table[t] = f

    def get_handler(self, obj: T) -> Callable[Concatenate[T, P], TReturn] | None:
        # type-ignores are used here because mypy can't tell we'll always
        # return a Callable[[T, P], TReturn] for any obj: T.
        # This is because we only ever store T: Callable[[T, P], TReturn] in _table.
        f = self._table.get(type(obj))
        if f is None:
            for t, f in self._table.items():
                if isinstance(obj, t):
                    return f
            return None
        else:
            return f

    def keys(self) -> Iterable[Type[Any]]:
        return self._table.keys()


TRenderer_contra = TypeVar("TRenderer_contra", bound="Renderer", contravariant=True)
TVisitorOutcome = TypeVar("TVisitorOutcome")

# TODO if i want to render Anchors, can i? it's not an inline... is it?


class RendererHandlers(Generic[TRenderer_contra]):
    block_inline_emitters: DynamicNodeDispatch[[TRenderer_contra, FormatContext], None]
    header_emitters: DynamicNodeDispatch[
        [BlockScope, Iterator[DocSegment], TRenderer_contra, FormatContext],
        None,
    ]

    def __init__(self) -> None:
        super().__init__()
        self.block_inline_emitters = DynamicNodeDispatch()
        self.header_emitters = DynamicNodeDispatch()

    def register_block_or_inline(
        self,
        type: Type[TBlockOrInline],
        renderer: Callable[[TBlockOrInline, TRenderer_contra, FormatContext], None],
    ) -> None:
        self.block_inline_emitters.register_handler(type, renderer)

    def register_header(
        self,
        type: Type[THeader],
        renderer: Callable[
            [
                THeader,
                BlockScope,
                Iterator[DocSegment],
                TRenderer_contra,
                FormatContext,
            ],
            None,
        ],
    ) -> None:
        self.header_emitters.register_handler(type, renderer)

    def emit_block_or_inline(
        self,
        n: TBlockOrInline,
        renderer: TRenderer_contra,
        fmt: FormatContext,
    ) -> None:
        f = self.block_inline_emitters.get_handler(n)
        if f is None:
            raise NotImplementedError(f"Didn't have renderer for {n}")
        f(n, renderer, fmt)

    def emit_doc_segment(
        self,
        s: DocSegment,
        renderer: TRenderer_contra,
        fmt: FormatContext,
    ) -> None:
        f = self.header_emitters.get_handler(s.header)
        if f is None:
            raise NotImplementedError(f"Didn't have renderer for {s.header}")
        f(s.header, s.contents, s.subsegments, renderer, fmt)

    def renderer_keys(self) -> Set[Type[Block | Inline | DocSegmentHeader]]:
        return set(self.block_inline_emitters.keys()).union(self.header_emitters.keys())


VisitorFilter = Tuple[Type[Any], ...] | Type[Any] | None
VisitorFunc = Callable[[Any], None]


class DocumentDfsPass:
    visitors: List[Tuple[VisitorFilter, VisitorFunc]]

    def __init__(self, visitors: List[Tuple[VisitorFilter, VisitorFunc]]) -> None:
        self.visitors = visitors

    def dfs_over_document(self, doc: Document) -> None:
        # Parse the floats in ""order""
        # type-ignore because this relies on covariance.
        # doc.floats.values() is a sequence of Block, [doc.toplevel] is a list of DocSegment
        dfs_queue: List[Block | Inline | DocSegment | DocSegmentHeader] = list(
            reversed(doc.floats.values())
        ) + [
            doc.toplevel
        ]  # type: ignore
        while dfs_queue:
            node = dfs_queue.pop()

            # Visit the node
            for v_type, v_f in self.visitors:
                if v_type is None or isinstance(node, v_type):
                    v_f(node)

            # Extract children as a reversed iterator.
            # reversed is important because we pop the last thing in the queue off first.
            children: Iterable[Block | Inline | DocSegment | DocSegmentHeader]
            if isinstance(node, (BlockScope, InlineScope)):
                children = reversed(tuple(node))
            elif isinstance(node, DocSegment):
                children = reversed((node.header, node.contents, *node.subsegments))
            elif node is None:
                children = None
            else:
                contents = getattr(node, "contents", None)
                children = reversed(list(contents)) if contents is not None else None  # type: ignore
            if children is not None:
                dfs_queue.extend(children)


class Writable(Protocol):
    def write(self, s: str, /) -> int:
        ...


class Renderer:
    doc: DocState
    fmt: FormatContext
    handlers: RendererHandlers  # type: ignore[type-arg]
    write_to: Writable

    _indent: str = ""
    # After emitting a newline with emit_newline, this is set.
    # The next call to emit_raw will emit _indent.
    # This is important if you want to change the indent after something has already emitted a newline,
    # e.g. if you wrap emit_paragraph() in indent(4), the emit_paragraph() will emit a final newline but *not* immediately emit the indent of 4, so subsequent emissions are nicely indented.
    # In the same way, if you emit a newline *then* change the indent, the next emitted item will have the new indent applied.
    _need_indent: bool = False

    def __init__(
        self,
        doc: DocState,
        fmt: FormatContext,
        handlers: RendererHandlers,  # type: ignore[type-arg]
        write_to: Writable,
    ) -> None:
        self.doc = doc
        self.fmt = fmt
        self.handlers = handlers
        self.write_to = write_to

    @classmethod
    def render_to_path(
        cls: Type[TRenderer_contra],
        plugins: Sequence["RenderPlugin[TRenderer_contra]"],
        doc: Document,
        write_to_path: Union[str, bytes, "os.PathLike[Any]"],
    ) -> None:
        with open(write_to_path, "w", encoding="utf-8") as write_to:
            cls.render(plugins, doc, write_to)

    @classmethod
    def render(
        cls: Type[TRenderer_contra],
        plugins: Sequence["RenderPlugin[TRenderer_contra]"],
        doc: Document,
        write_to: Writable | None = None,
    ) -> io.StringIO | None:
        if write_to is None:
            write_to = io.StringIO()

        handlers: RendererHandlers[TRenderer_contra] = RendererHandlers()
        handlers.register_block_or_inline(
            BlockScope, lambda bs, r, fmt: r.emit_blockscope(bs)
        )
        handlers.register_block_or_inline(
            Paragraph, lambda p, r, fmt: r.emit_paragraph(p)
        )
        handlers.register_block_or_inline(
            InlineScope, lambda inls, r, fmt: r.emit_inlinescope(inls)
        )
        handlers.register_block_or_inline(
            UnescapedText, lambda t, r, fmt: r.emit_unescapedtext(t)
        )
        for p in plugins:
            p._register_node_handlers(handlers)

        missing_renderers = doc.exported_nodes.difference(handlers.renderer_keys())
        if missing_renderers:
            raise RuntimeError(
                f"Some node types were not given renderers by any plugin, but are used by the document: {missing_renderers}"
            )

        # TODO reenable this
        # missing_doc_counters = doc.counted_anchor_kinds.difference(
        #     counters.anchor_kinds()
        # )
        # if missing_doc_counters:
        #     raise RuntimeError(
        #         f"Some counters are not declared in the CounterSet, but are used by the document: {missing_doc_counters}"
        #     )

        # The (first?) visitor/counter pass
        dfs_visitors: List[Tuple[VisitorFilter, VisitorFunc]] = []
        for p in plugins:
            new_visitors = p._make_visitors()
            if new_visitors:
                dfs_visitors.extend(new_visitors)
        dfs_pass = DocumentDfsPass(dfs_visitors)

        dfs_pass.dfs_over_document(doc)

        # The rendering pass
        renderer = cls(doc.doc, doc.fmt, handlers, write_to)
        renderer.emit_segment(doc.toplevel)

        if isinstance(write_to, io.StringIO):
            return write_to
        return None

    def emit_raw(self, x: str) -> None:
        """
        The function on which all emitters are based.
        """
        if self._need_indent:
            self.write_to.write(self._indent)
            self._need_indent = False
        self.write_to.write(x)

    def emit_newline(self) -> None:
        self.write_to.write("\n")
        self._need_indent = True

    # TODO pass a generator instead of emit_t, ts!
    def emit_join(
        self,
        emit_t: Callable[[T], None],
        ts: Iterable[T],
        emit_join: Callable[[], None],
    ) -> None:
        first = True
        for t in ts:
            if not first:
                emit_join()
            first = False
            emit_t(t)

    def emit_join_gen(
        self, emit_gen: Generator[None, None, None], emit_join: Callable[[], None]
    ) -> None:
        first = True
        while True:
            if not first:
                emit_join()
            first = False
            try:
                next(emit_gen)
            except StopIteration:
                break

    def emit_break_sentence(self) -> None:
        self.emit_newline()

    def emit_break_paragraph(self) -> None:
        self.emit_newline()
        self.emit_newline()

    @abc.abstractmethod
    def emit_unescapedtext(self, t: UnescapedText) -> None:
        """
        Given some text, emit a string that will look like that text exactly in the given backend.
        """
        raise NotImplementedError(f"Need to implement emit_unescapedtext")

    # TODO this is probably a bad idea to implement because it will get mixed up with raw.
    # def emit(self, x: Any) -> None:
    #     if isinstance(x, Inline):
    #         self.emit_inline(x)
    #     else:
    #         self.emit_block(x)

    # TODO or i could get even crazier with it - make it expand tuples?
    def emit(self, *args: Any, joiner: Optional[Callable[[], None]] = None) -> None:
        first = True
        for a in args:
            if joiner and not first:
                joiner()
            first = False
            if isinstance(a, str):
                self.emit_raw(a)
            elif isinstance(a, Inline):
                self.emit_inline(a)
            elif isinstance(a, DocSegment):
                self.emit_segment(a)
            elif isinstance(a, Block):
                self.emit_block(a)
            else:
                raise ValueError(f"Don't know how to automatically render {a}")

    def emit_inline(self: Self, i: Inline) -> None:
        self.handlers.emit_block_or_inline(i, self, self.fmt)

    def emit_block(self: Self, b: Block) -> None:
        self.handlers.emit_block_or_inline(b, self, self.fmt)

    def emit_segment(self: Self, s: DocSegment) -> None:
        if s.header is None:
            self.emit_blockscope(s.contents)
            self.emit(*s.subsegments)
        else:
            self.emit_break_paragraph()
            self.handlers.emit_doc_segment(
                s,
                self,
                self.fmt,
            )

    def emit_blockscope(self, bs: BlockScope) -> None:
        # Default: join paragraphs with self.PARAGRAPH_SEP
        # If you get nested blockscopes, this will still be fine - you won't get double separators
        self.emit_join(self.emit_block, bs, self.emit_break_paragraph)

    def emit_paragraph(self, p: Paragraph) -> None:
        # Default: join sentences with self.SENTENCE_SEP
        self.emit_join(self.emit_sentence, p, self.emit_break_sentence)

    def emit_inlinescope(self, inls: InlineScope) -> None:
        # Default: join internal inline elements directly
        for i in inls:
            self.emit_inline(i)

    def emit_sentence(self, s: Sentence) -> None:
        # Default: join internal inline elements directly
        # TODO could be extended by e.g. latex to ensure you get sentence-break-whitespace at the end of each sentence?
        for i in s:
            self.emit_inline(i)

    def push_indent(self, n: int) -> None:
        self._indent += " " * n

    def pop_indent(self, n: int) -> None:
        if len(self._indent) < n:
            raise ValueError()
        self._indent = self._indent[:-n]

    @contextmanager
    def indent(self, n: int) -> Iterator[None]:
        self.push_indent(n)
        try:
            yield
        finally:
            self.pop_indent(n)


class RenderPlugin(Generic[TRenderer_contra]):
    @abc.abstractmethod
    def _register_node_handlers(
        self, handlers: RendererHandlers[TRenderer_contra]
    ) -> None:
        raise NotImplementedError()

    # TODO make this include serial passes, not parallel? is that useful?
    def _make_visitors(self) -> Optional[List[Tuple[VisitorFilter, VisitorFunc]]]:
        return None
