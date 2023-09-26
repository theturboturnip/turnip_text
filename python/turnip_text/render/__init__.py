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
from turnip_text.doc import Document, FormatContext
from turnip_text.doc.anchors import Anchor
from turnip_text.render.counters import CounterSet

T = TypeVar("T")
P = ParamSpec("P")
TReturn = TypeVar("TReturn")
TNode = TypeVar("TNode", Block, Inline)


class DynamicNodeDispatch(Generic[P, TReturn]):
    _table: Dict[
        Type[Block | Inline | DocSegmentHeader], Union[
            Callable[Concatenate[Block, P], TReturn],
            Callable[Concatenate[Inline, P], TReturn],
            Callable[Concatenate[DocSegmentHeader, P], TReturn],
        ]
    ]

    def __init__(self) -> None:
        super().__init__()
        self._table = {}

    def register_handler(
        self,
        t: Type[TNode],
        f: Callable[Concatenate[TNode, P], TReturn],
    ) -> None:
        if t in self._table:
            raise RuntimeError(f"Conflict: registered two handlers for {t}")
        # We know that we only assign _table[t] if f takes t, and that when we pull it
        # out we will always call f with something of type t.
        # mypy doesn't know that, so we say _table stores functions taking T (the base class)
        # and sweep the difference under the rug
        self._table[t] = f

    def get_handler(self, obj: TNode) -> Callable[Concatenate[TNode, P], TReturn] | None:
        # type-ignores are used here because mypy can't tell we'll always 
        # return a Callable[[T, P], TReturn] for any obj: T.
        # This is because we only ever store T: Callable[[T, P], TReturn] in _table.
        f = self._table.get(type(obj))
        if f is None:
            for t, f in self._table.items():
                if isinstance(obj, t):
                    return f # type: ignore
            return None
        else:
            return f # type: ignore
        
    def keys(self) -> Iterable[Type[Block | Inline | DocSegmentHeader]]:
        return self._table.keys()
    
TRenderer_contra = TypeVar("TRenderer_contra", bound="Renderer", contravariant=True)
TVisitorOutcome = TypeVar("TVisitorOutcome")

class RendererHandlers(Generic[TRenderer_contra]):
    visitors: DynamicNodeDispatch[[], Any]
    emitters: DynamicNodeDispatch[[Any, TRenderer_contra, FormatContext], None]

    def register_node(
        self,
        type: Type[TNode],
        visitor: Callable[[TNode], TVisitorOutcome],
        renderer: Callable[[TNode, TVisitorOutcome, TRenderer_contra, FormatContext], None]
    ) -> None:
        self.visitors.register_handler(type, visitor)
        self.emitters.register_handler(type, renderer)

    def register_node_visitor(
        self,
        type: Type[TNode],
        visitor: Callable[[TNode], None],
    ) -> None:
        self.visitors.register_handler(type, visitor)

    def register_node_renderer(
        self,
        type: Type[TNode],
        renderer: Callable[[TNode, None, TRenderer_contra, FormatContext], None]
    ) -> None:
        self.emitters.register_handler(type, renderer)

    def visit(self, n: Block | Inline | DocSegmentHeader) -> Any:
        f = self.visitors.get_handler(n) # type: ignore
        if f is None:
            return None
        return f(n)

    def emit(self, n: TNode, v: TVisitorOutcome, renderer: TRenderer_contra, fmt: FormatContext) -> None:
        f = self.emitters.get_handler(n)
        if f is None:
            raise NotImplementedError(f"Didn't have renderer for {n}")
        f(n, v, renderer, fmt)

class Writable(Protocol):
    def write(self, s: str, /) -> int:
        ...

class Renderer:
    fmt: FormatContext
    handlers: RendererHandlers # type: ignore[type-arg]
    visit_results: Dict[Block | Inline | DocSegmentHeader, Any]
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
            fmt: FormatContext,
            handlers: RendererHandlers, # type: ignore[type-arg]
            visit_results: Dict[Block | Inline | DocSegmentHeader, Any],
            write_to: Writable
        ) -> None:
        self.fmt = fmt
        self.handlers = handlers
        self.visit_results = visit_results
        self.write_to = write_to

    @classmethod
    def render_to_path(
        cls: Type[TRenderer_contra],
        plugins: Sequence["RenderPlugin[TRenderer_contra]"],
        counters: CounterSet,
        doc: Document,
        write_to_path: str | bytes | "os.PathLike[Any]"
    ) -> None:
        with open(write_to_path, "w", encoding="utf-8") as write_to:
            cls.render(plugins, counters, doc, write_to)

    @classmethod
    def render(
        cls: Type[TRenderer_contra],
        plugins: Sequence["RenderPlugin[TRenderer_contra]"],
        counters: CounterSet,
        doc: Document,
        write_to: Writable | None=None
    ) -> io.StringIO | None:
        if write_to is None:
            write_to = io.StringIO()

        handlers: RendererHandlers[TRenderer_contra] = RendererHandlers()
        handlers.register_node_renderer(
            BlockScope, lambda bs, _, fmt, r: r.emit_blockscope(bs)
        )
        handlers.register_node_renderer(
            Paragraph, lambda p, _, fmt, r: r.emit_paragraph(p)
        )
        handlers.register_node_renderer(
            InlineScope, lambda inls, _, fmt, r: r.emit_inlinescope(inls)
        )
        handlers.register_node_renderer(
            UnescapedText, lambda t, _, fmt, r: r.emit_unescapedtext(t)
        )
        for p in plugins:
            p._register_node_handlers(handlers)

        missing_renderers = doc.exported_nodes.difference(handlers.emitters.keys())
        if missing_renderers:
            raise RuntimeError(f"Some node types were not given renderers by any plugin, but are used by the document: {missing_renderers}")

        missing_doc_counters = doc.counted_anchor_kinds.difference(counters.anchor_kinds())
        if missing_doc_counters:
            raise RuntimeError(f"Some counters are not declared in the CounterSet, but are used by the document: {missing_doc_counters}")

        # The visitor/counter pass
        visit_results: Dict[Block | Inline | DocSegmentHeader, Any] = {}
        anchor_counters: Dict[Anchor, Tuple[UnescapedText, ...]] = {}
        dfs_queue: List[Block | Inline | DocSegment | DocSegmentHeader] = [doc.toplevel]
        while dfs_queue:
            node = dfs_queue.pop()

            # Counter pass
            anchor = getattr(node, "anchor", None)
            if isinstance(anchor, Anchor):
                anchor_counters[anchor] = counters.increment_counter(anchor.kind)

            # Visit the node
            if not isinstance(node, DocSegment):
                visit_result = handlers.visit(node)
                if visit_result is not None:
                    visit_results[node] = visit_result

            # Extract children as a reversed iterator.
            # reversed is important because we pop the last thing in the queue off first.
            children: Iterable[Block | Inline | DocSegment | DocSegmentHeader]
            if isinstance(node, (BlockScope, InlineScope)):
                children = reversed(tuple(*node))
            elif isinstance(node, DocSegment):
                children = reversed((node.header, node.contents, *node.subsegments))
            elif node is None:
                children = None
            else:
                children = reversed(getattr(node, "contents", None)) # type: ignore
            if children is not None:
                dfs_queue.extend(children)

        # The rendering pass
        renderer = cls(
            doc.fmt,
            handlers,
            visit_results,
            write_to
        )
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
    def emit_join(self, emit_t: Callable[[T], None], ts: Iterable[T], emit_join: Callable[[], None]) -> None:
        first = True
        for t in ts:
            if not first:
                emit_join()
            first = False
            emit_t(t)

    def emit_join_gen(self, emit_gen: Generator[None, None, None], emit_join: Callable[[], None]) -> None:
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
    def emit(self, *args: Any, joiner: Optional[Callable[[], None]]=None) -> None:
        first = True            
        for a in args:
            if joiner and not first:
                joiner()
            first = False
            if isinstance(a, str):
                self.emit_raw(a)
            elif isinstance(a, Inline):
                self.emit_inline(a)
            else:
                self.emit_block(a)

    def emit_inline(self: Self, i: Inline) -> None:
        self.handlers.emit(i, self.visit_results.get(i, None), self, self.fmt)

    def emit_block(self: Self, b: Block) -> None:
        self.handlers.emit(b, self.visit_results.get(b, None), self, self.fmt)

    def emit_segment(self: Self, s: DocSegment) -> None:
        # TODO need to sort out how to render segments. You don't render the segments, 
        self.handlers.emit(s, self.visit_results.get(s, None), self, self.fmt)

    def emit_blockscope(self, bs: BlockScope) -> None:
        # Default: join paragraphs with self.PARAGRAPH_SEP
        # If you get nested blockscopes, this will still be fine - you won't get double separators
        self.emit_join(self.emit_block, bs, self.emit_break_paragraph)

    def emit_paragraph(self, p: Paragraph) -> None:
        # Default: join sentences with self.SENTENCE_SEP
        for s in p:
            self.emit_sentence(s)

    def emit_inlinescope(self, inls: InlineScope) -> None:
        # Default: join internal inline elements directly
        for i in inls:
            self.emit_inline(i)

    def emit_sentence(self, s: Sentence) -> None:
        # Default: join internal inline elements directly
        # TODO could be extended by e.g. latex to ensure you get sentence-break-whitespace at the end of each sentence?
        for i in s:
            self.emit_inline(i)
        # TODO this shouldn't be here, surely. it should be in emit_paragraph, *joining* sentences instead of *ending* them
        self.emit_break_sentence()

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
    def _register_node_handlers(self, handlers: RendererHandlers[TRenderer_contra]) -> None:
        raise NotImplementedError()