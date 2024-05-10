import abc
from contextlib import contextmanager
from typing import (
    Callable,
    Dict,
    Generator,
    Generic,
    Iterable,
    Iterator,
    List,
    Optional,
    Protocol,
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
    Document,
    Header,
    Inline,
    InlineScope,
    Paragraph,
    Raw,
    Sentence,
    Text,
)
from turnip_text.build_system import BuildSystem
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.doc.dfs import VisitorFilter, VisitorFunc
from turnip_text.env_plugins import EnvPlugin, FmtEnv
from turnip_text.plugins.anchors import StdAnchorPlugin
from turnip_text.render.dyn_dispatch import DynDispatch

T = TypeVar("T")
TBlockOrInline = TypeVar("TBlockOrInline", bound=Union[Block, Inline])
THeader = TypeVar("THeader", bound=Header)
TVisitable = TypeVar("TVisitable", bound=Union[Block, Inline, Header])
TRenderer = TypeVar("TRenderer", bound="Renderer")
TRenderer_contra = TypeVar("TRenderer_contra", bound="Renderer", contravariant=True)
TVisitorOutcome = TypeVar("TVisitorOutcome")


class RefEmitterDispatch(Generic[TRenderer_contra]):
    """Performs dynamic dispatch for anchor/backref rendering *technology*.

    This covers the renderer-specific mechanics of how an anchor is created and referred to, *not* how it is counted or named.
    """

    anchor_kind_to_method: Dict[str, str]

    anchor_default: Optional[Callable[[TRenderer_contra, "FmtEnv", Anchor], None]]
    backref_default: Optional[Callable[[TRenderer_contra, "FmtEnv", Backref], None]]

    anchor_table: Dict[str, Callable[[TRenderer_contra, "FmtEnv", Anchor], None]]
    backref_table: Dict[str, Callable[[TRenderer_contra, "FmtEnv", Backref], None]]

    def __init__(self) -> None:
        super().__init__()
        self.anchor_kind_to_method = {}
        self.anchor_default = None
        self.backref_default = None
        self.anchor_table = {}
        self.backref_table = {}

    def register_anchor_render_method(
        self,
        method: str,
        anchor: Callable[[TRenderer_contra, "FmtEnv", Anchor], None],
        backref: Callable[[TRenderer_contra, "FmtEnv", Backref], None],
        can_be_default: bool = True,
    ) -> None:
        if method in self.anchor_table:
            raise RuntimeError(
                f"Conflict: registered two anchor rendering functions for method '{method}'"
            )
        self.anchor_table[method] = anchor
        self.backref_table[method] = backref
        if can_be_default and (self.anchor_default is None):
            self.anchor_default = anchor
            self.backref_default = backref

    def request_method_for_anchor_kind(self, anchor_kind: str, method: str) -> None:
        if anchor_kind in self.anchor_kind_to_method:
            raise ValueError(
                "Conflict: requested two rendering methods for anchor kind '{anchor_kind}'"
            )
        self.anchor_kind_to_method[anchor_kind] = method

    def get_anchor_emitter(
        self, a: Anchor
    ) -> Callable[[TRenderer_contra, "FmtEnv", Anchor], None]:
        method = self.anchor_kind_to_method.get(a.kind)
        if method is None:
            if self.anchor_default is None:
                raise RuntimeError(
                    f"Couldn't find a fallback emitter function for anchor kind '{a.kind}' - no default registered"
                )
            return self.anchor_default
        return self.anchor_table[method]

    def get_backref_emitter(
        self, backref_kind: str
    ) -> Callable[[TRenderer_contra, "FmtEnv", Backref], None]:
        method = self.anchor_kind_to_method.get(backref_kind)
        if method is None:
            if self.backref_default is None:
                raise RuntimeError(
                    f"Couldn't find a fallback emitter function for anchor kind '{backref_kind}' - no default registered"
                )
            return self.backref_default
        return self.backref_table[method]


class EmitterDispatch(Generic[TRenderer_contra]):
    """Performs DynDispatch for block, inline, and header emitters"""

    block_inline_emitters: DynDispatch[[TRenderer_contra, "FmtEnv"], None]
    header_emitters: DynDispatch[
        [BlockScope, Iterator[DocSegment], TRenderer_contra, "FmtEnv"],
        None,
    ]

    def __init__(self) -> None:
        super().__init__()
        self.block_inline_emitters = DynDispatch()
        self.header_emitters = DynDispatch()

    def register_block_or_inline(
        self,
        type: Type[TBlockOrInline],
        renderer: Callable[[TBlockOrInline, TRenderer_contra, "FmtEnv"], None],
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
                FmtEnv,
            ],
            None,
        ],
    ) -> None:
        self.header_emitters.register_handler(type, renderer)

    def emit_block_or_inline(
        self,
        n: TBlockOrInline,
        renderer: TRenderer_contra,
        fmt: FmtEnv,
    ) -> None:
        f = self.block_inline_emitters.get_handler(n)
        if f is None:
            raise NotImplementedError(f"Didn't have renderer for {n}")
        f(n, renderer, fmt)

    def emit_doc_segment(
        self,
        s: DocSegment,
        renderer: TRenderer_contra,
        fmt: "FmtEnv",
    ) -> None:
        f = self.header_emitters.get_handler(s.header)
        if f is None:
            raise NotImplementedError(f"Didn't have renderer for {s.header}")
        f(s.header, s.contents, s.subsegments, renderer, fmt)

    def renderer_keys(self) -> Set[Type[Block | Inline | Header]]:
        return set(self.block_inline_emitters.keys()).union(self.header_emitters.keys())


class Writable(Protocol):
    def write(self, s: str, /) -> int: ...


class Renderer(abc.ABC):
    fmt: FmtEnv
    anchors: StdAnchorPlugin
    handlers: EmitterDispatch  # type: ignore[type-arg]
    write_to: Writable

    _indent: str = ""
    # After emitting a newline with emit_newline, this is set.
    # The next call to emit_raw will emit _indent.
    # This is important if you want to change the indent after something has already emitted a newline,
    # e.g. if you wrap emit_paragraph() in indent(4), the emit_paragraph() will emit a final newline but *not* immediately emit the indent of 4, so subsequent emissions are nicely indented.
    # In the same way, if you emit a newline *then* change the indent, the next emitted item will have the new indent applied.
    _need_indent: bool = False

    def __init__(
        self: TRenderer,
        fmt: FmtEnv,
        anchors: StdAnchorPlugin,
        handlers: EmitterDispatch[TRenderer],
        write_to: Writable,
    ) -> None:
        self.fmt = fmt
        self.anchors = anchors
        self.handlers = handlers
        self.write_to = write_to

    @classmethod
    def default_emitter_dispatch(
        cls: Type[TRenderer],
    ) -> EmitterDispatch[TRenderer]:
        """This is a convenience method that generates the most basic EmitterDispatch for a renderer. It is meant to be called by RenderSetup classes. It can be overridden in renderers that provide more than the basic emitters."""
        handlers: EmitterDispatch[TRenderer] = EmitterDispatch()
        handlers.register_block_or_inline(
            BlockScope, lambda bs, r, fmt: r.emit_blockscope(bs)
        )
        handlers.register_block_or_inline(
            Paragraph, lambda p, r, fmt: r.emit_paragraph(p)
        )
        handlers.register_block_or_inline(
            InlineScope, lambda inls, r, fmt: r.emit_inlinescope(inls)
        )
        handlers.register_block_or_inline(Text, lambda t, r, fmt: r.emit_text(t))
        handlers.register_block_or_inline(Raw, lambda t, r, fmt: r.emit_raw(t.data))
        # handlers.register_block_or_inline(
        #     Anchor, lambda a, r, fmt: r.ref_handler.get_anchor_emitter(a)(r, fmt, a)
        # )
        # handlers.register_block_or_inline(
        #     Backref,
        #     lambda b, r, fmt: r.ref_handler.get_backref_emitter(
        #         r.anchors.lookup_backref(b).kind
        #     )(r, fmt, b),
        # )

        # ref_handlers: RefEmitterDispatch[TRenderer] = RefEmitterDispatch()

        return handlers

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
    def emit_text(self, t: Text) -> None:
        """
        Given some text, emit a string that will look like that text exactly in the given backend.
        """
        raise NotImplementedError(f"Need to implement emit_text")

    def emit(
        self,
        *args: Union[Inline, Block, DocSegment],
        joiner: Optional[Callable[[], None]] = None,
    ) -> None:
        first = True
        for a in args:
            if joiner and not first:
                joiner()
            first = False
            if isinstance(a, Inline):
                self.emit_inline(a)
            elif isinstance(a, Block):
                self.emit_block(a)
            elif isinstance(a, DocSegment):
                self.emit_segment(a)
            else:
                raise ValueError(f"Don't know how to automatically render {a}")

    def emit_inline(self, i: Inline) -> None:
        self.handlers.emit_block_or_inline(i, self, self.fmt)

    def emit_block(self, b: Block) -> None:
        self.handlers.emit_block_or_inline(b, self, self.fmt)

    # This can be overridden by renderers to add stuff at the top level
    def emit_document(self, doc: Document) -> None:
        self.emit_blockscope(doc.contents)
        self.emit(*doc.segments)

    def emit_segment(self, s: DocSegment) -> None:
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


# Can't specify generic type bounds inside a TypeVar bound, so RenderSetup doesn't have a generic here
# Contravariant so that if RenderSetupB subclasses RenderSetupA, i.e. RenderSetupB provides the same features as RenderSetupA, RenderPlugin[RenderSetupA] can be passed into a (self: RenderSetupB, plugins: Iterable[RenderPlugin[TRenderer, RenderSetupB]]) i.e. RenderPlugin[T, RenderSetupA] is considered a subtype of RenderPlugin[T, RenderSetupB].
# See https://peps.python.org/pep-0483/#covariance-and-contravariance
TRenderSetup = TypeVar("TRenderSetup", bound="RenderSetup", contravariant=True)  # type: ignore[type-arg]


class RenderSetup(abc.ABC, Generic[TRenderer]):
    plugins: Iterable["RenderPlugin"]  # type: ignore[type-arg]

    def register_plugins(
        self: TRenderSetup,
        build_sys: BuildSystem,
        plugins: Iterable["RenderPlugin[TRenderSetup]"],
    ) -> None:
        self.plugins = list(plugins)
        for plugin in plugins:
            plugin._register(build_sys, self)

    @abc.abstractmethod
    def gen_dfs_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]]: ...

    @abc.abstractmethod
    def known_node_types(
        self,
    ) -> Iterable[Type[Union[Block, Inline, Header]]]: ...

    @abc.abstractmethod
    def known_countables(self) -> Iterable[str]: ...

    @abc.abstractmethod
    def register_file_generator_jobs(
        self,
        fmt: FmtEnv,
        anchors: StdAnchorPlugin,
        document: Document,
        build_sys: BuildSystem,
        output_file_name: Optional[str],
    ) -> None:
        """Register the actual job to render the necessary files out from toplevel_segment into doc_setup.build_sys."""
        ...


# I have previously been worried about what happens if a renderer-agnostic subclass of EnvPlugin
# (for example, NamedUrlPlugin which is defined as system-agnostic at the top level)
# is then combined with RenderPlugin.
# This would be fine, because RenderPlugin is a *disjoint* subclass of EnvPlugin.
# In other languages this would result in a diamond problem, where you might have
# multiple "copies" of the same superclass inside the subclass, but Python doesn't work that way.
#
# If it overrode any EnvPlugin function, you'd get a conflict between the RenderPlugin version
# of that function and the e.g. NamedUrlPlugin version. That conflict would be solved according
# to the Method Resolution Order, which itself relies on the ordering of subclassing.
# Subclass1(RenderPlugin, NamedUrlPlugin) and Subclass2(NamedUrlPlugin, RenderPlugin) would have *different*
# method resolutions, the leftmost superclass is picked for method resolution first.
#
# As it stands, if you subclass both the Method Resolution Order will be Subclass, (RenderPlugin, NamedUrlPlugin or vice versa), EnvPlugin.
# NamedUrlPlugin will always come before EnvPlugin, so all the NamedUrl methods will be as expected.
# Because RenderPlugin is disjoint from EnvPlugin, and we can assume NamedUrlPlugin is disjoint from RenderPlugin, the methods will always resolve as one expects. No diamond problem.
class RenderPlugin(Generic[TRenderSetup], EnvPlugin):
    def _register(self, build_sys: BuildSystem, setup: TRenderSetup) -> None:
        return None

    # Return a list of (filter, visitor) functions which are run in parallel over a single DFS pass on the frozen document.
    # Right now there are no usecases for emitting serial sets of DFS passes, because these fundamentally don't mutate state.
    # If you have some complex computation on the state of the document, you can glean all necessary information up front and then do the computation.
    # TODO make this empty-list instead of None
    def _make_visitors(self) -> Optional[List[Tuple[VisitorFilter, VisitorFunc]]]:
        return None
