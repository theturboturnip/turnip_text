import re
from typing import (
    Any,
    Callable,
    Dict,
    Generic,
    Iterable,
    List,
    Optional,
    Self,
    Sequence,
    Tuple,
    Type,
    TypeVar,
    Union,
)

import pandoc  # type:ignore

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
from turnip_text.build_system import BuildSystem, OutputRelPath, RelPath
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.doc.dfs import VisitorFilter, VisitorFunc
from turnip_text.env_plugins import FmtEnv, THeader
from turnip_text.plugins.anchors import StdAnchorPlugin
from turnip_text.render import Renderer, RenderPlugin, RenderSetup
from turnip_text.render.counters import (
    CounterLink,
    CounterState,
    build_counter_hierarchy,
)
from turnip_text.render.dyn_dispatch import DynDispatch
from turnip_text.render.manual_numbering import SimpleCounterFormat, SimpleCounterStyle

from . import pandoc_types as pan

T = TypeVar("T")
TBlock = TypeVar("TBlock", bound=Block)
TInline = TypeVar("TInline", bound=Inline)
TPandocRenderer_contra = TypeVar(
    "TPandocRenderer_contra", bound="PandocRenderer", contravariant=True
)


# Helper functions for making pan.Attr
def pan_attr(id: str, props: List[str], keyval: List[Tuple[str, str]]) -> pan.Attr:
    return (id, props, keyval)


def null_attr() -> pan.Attr:
    return ("", [], [])


# Helper functions for making pan.Meta from JSON
PanJson = Union[int, float, str, List["PanJson"], Dict[str, "PanJson"]]


def map_json_to_pan_metavalue(obj: PanJson) -> pan.MetaValue:
    if isinstance(obj, (int, float, str)):
        return pan.MetaInlines(PandocRenderer.make_text_inline_list(Text(str(obj))))
    elif isinstance(obj, list):
        return pan.MetaList([map_json_to_pan_metavalue(elem) for elem in obj])
    else:
        return pan.MetaMap(
            {key: map_json_to_pan_metavalue(value) for key, value in obj.items()}
        )


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
    """
    An implementation of Renderer that builds a `pandoc.Document` which can then be processed into arbitrary output formats.

    Pandoc requires an AST, so this renderer is "maker"-based - plugins need to register functions that turn turnip_text Blocks into pandoc Blocks, and turnip_text Inlines into pandoc Inlines.

    Each turnip_text counter is one of:
    1. turnip_text controlled, Pandoc unaware
        - turnip_text has to render the counter
    2. Pandoc controlled, turnip_text mimicked (e.g. section headings)
        - Legal/required for turnip_text to render the counter, it should always match exactly
        - In the case of section headings, Pandoc always uses arabic numbering - turnip_text has to match
    3. Pandoc controlled, turnip_text doesn't render (e.g. footnotes)
        - Illegal/unadvised for turnip_text to render the counter,
        it won't necessarily get the numbers right

    1. and 2. counters have non-None entries in self.counter_rendering, and are supported in self.make_anchor(), self.make_backref(), and the anchor-text generators.
    3. counters have None entries and aren't supported.
    """

    meta: pan.Meta
    makers: "PandocDispatch[Self]"
    counters: CounterState
    counter_rendering: Dict[str, Optional[SimpleCounterFormat[SimpleCounterStyle]]]
    """
    A mapping of counters to how they are rendered.
    Some counters are Pandoc-controlled and thus not directly renderable, and these cannot be parents of Pandoc-independent, renderable counters.
    """
    pandoc_options: List[str]
    """Options to be passed to pandoc on the command line"""

    def __init__(
        self,
        fmt: FmtEnv,
        anchors: StdAnchorPlugin,
        meta: pan.Meta,
        makers: "PandocDispatch[Self]",
        counters: CounterState,
        counter_rendering: Dict[str, Optional[SimpleCounterFormat[SimpleCounterStyle]]],
        pandoc_options: List[str],
    ) -> None:
        super().__init__(fmt, anchors)
        self.meta = meta
        self.makers = makers
        self.counters = counters
        self.counter_rendering = counter_rendering
        self.pandoc_options = pandoc_options

    @classmethod
    def default_makers(
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
        dispatch.register_inline(Backref, lambda b, r, fmt: r.make_backref(b))
        dispatch.register_inline(Text, lambda t, r, fmt: r.make_text(t))
        dispatch.register_inline(Raw, lambda raw, r, fmt: r.make_raw(raw))
        return dispatch

    def make_document(self: Self, doc: Document) -> pan.Pandoc:
        blocks = [self.make_block(b) for b in doc.contents]
        for seg in doc.segments:
            self.make_docsegment(seg, blocks)
        return pan.Pandoc(self.meta, blocks)

    def make_docsegment(
        self: Self, docsegment: DocSegment, blocks: List[pan.Block]
    ) -> None:
        blocks.append(self.make_header(docsegment.header))
        blocks.extend(self.make_block(b) for b in docsegment.contents)
        for subseg in docsegment.subsegments:
            self.make_docsegment(subseg, blocks)

    def make_header(self: Self, obj: THeader) -> pan.Header:
        return self.makers.make_pan_header(obj, self, self.fmt)

    def make_block(self: Self, obj: TBlock) -> pan.Block:
        return self.makers.make_pan_block(obj, self, self.fmt)

    def make_inline(self: Self, obj: TInline) -> pan.Inline:
        return self.makers.make_pan_inline(obj, self, self.fmt)

    # If we absolutely have to, emit a block scope as a single block
    def make_block_scope(self: Self, bs: Iterable[Block]) -> pan.Div:
        return pan.Div(null_attr(), self.make_block_scope_list(bs))

    # In all other cases it's better to make the block scope as a list instead
    def make_block_scope_list(self: Self, bs: Iterable[Block]) -> List[pan.Block]:
        return [self.make_block(b) for b in bs]

    def make_paragraph(self: Self, p: Paragraph) -> pan.Para:
        inls: List[pan.Inline] = []
        for sentence in p:
            inls.extend(self.make_inline(inl) for inl in sentence)
            inls.append(pan.SoftBreak())
        return pan.Para(inls)

    # As for block scopes, Pandoc likes to request List[pan.Inline]
    # and a direct route from InlineScope is nice
    def make_inline_scope(self, inls: Iterable[Inline]) -> pan.Span:
        return pan.Span(null_attr(), self.make_inline_scope_list(inls))

    def make_inline_scope_list(self: Self, inls: Iterable[Inline]) -> List[pan.Inline]:
        return [self.make_inline(inl) for inl in inls]

    def make_text(self, text: Text) -> pan.Inline:
        words = self.make_text_inline_list(text)
        if len(words) == 1:
            return words[0]
        else:
            return pan.Span(null_attr(), words)

    @classmethod
    def make_text_inline_list(cls, text: Text) -> List[pan.Inline]:
        """
        Unpack turnip_text Text to a list of pandoc Inline.

        turnip_text Text can have multiple words,
        pandoc Inline have words separated by Space
        """
        # Unsure how to handle non-breaking space, so passthrough the unicode character directly for now.
        # However, breaking space is counted as inter-word space.
        words = [pan.Str(word) for word in re.split("\s+", text.text)]
        return generic_join(words, pan.Space())

    def make_raw(self, raw: Raw) -> pan.Inline:
        raise TypeError("Cannot emit Raw into pandoc! It doesn't make sense!")

    def anchor_to_ref_text(self, anchor: Anchor) -> Text:
        if self.counter_rendering[anchor.kind] is None:
            raise ValueError(
                f"Counter '{anchor.kind}' is pandoc-controlled and we cannot directly create text references to it."
            )
        counters = self.counters.anchor_counters[anchor]
        return SimpleCounterFormat.resolve(
            # if self.counter_rendering[kind] is not None, the rendering for parent kinds won't be None either.
            [(self.counter_rendering[kind], i) for (kind, i) in counters]  # type:ignore
        )

    def anchor_to_number_text(self, anchor: Anchor) -> Text:
        if self.counter_rendering[anchor.kind] is None:
            raise ValueError(
                f"Counter '{anchor.kind}' is pandoc-controlled and we cannot directly create text references to it."
            )
        counters = self.counters.anchor_counters[anchor]
        return SimpleCounterFormat.resolve(
            # if self.counter_rendering[kind] is not None, the rendering for parent kinds won't be None either.
            [
                (self.counter_rendering[kind], i)  # type:ignore
                for (kind, i) in counters
            ],
            with_name=False,
        )

    def make_anchor_attr(self, anchor: Optional[Anchor]) -> pan.Attr:
        # TODO raise error if counter is pandoc-controlled and unrenderable
        if anchor:
            return (anchor.canonical(), [], [])
        else:
            return ("", ["unnumbered"], [])

    def make_backref(self, backref: Backref) -> pan.Link:
        # e.g. Link(
        #   ("", [], [("reference-type", "ref"), ("reference", anchor.canonical())]),
        #   [Str("Name ") + (number)],
        #   ("#" + anchor.canonical(), "")
        # )
        # Pandoc AST requires manual numbering!!!
        # Autoref technically exists ("reference-type", "autoref") but in all my testing it never actually translates to an autoref.
        # Even if you take a LaTeX "\autoref" and put it through pandoc to LaTeX again it gets translated to \hyperlink{[anchor.canonoical()]}. Useless.

        # Numbering is one of

        anchor = self.anchors.lookup_backref(backref)
        return pan.Link(
            pan_attr(
                "", [], [("reference-type", "ref"), ("reference", anchor.canonical())]
            ),
            self.make_text_inline_list(self.anchor_to_ref_text(anchor)),
            (f"#{anchor.canonical()}", ""),
        )


# TODO makers should return List of items
class PandocDispatch(Generic[TPandocRenderer_contra]):
    header_makers: DynDispatch[["TPandocRenderer_contra", FmtEnv], pan.Header]
    block_makers: DynDispatch[["TPandocRenderer_contra", FmtEnv], pan.Block]
    inline_makers: DynDispatch[["TPandocRenderer_contra", FmtEnv], pan.Inline]

    def __init__(self) -> None:
        super().__init__()
        self.header_makers = DynDispatch()
        self.block_makers = DynDispatch()
        self.inline_makers = DynDispatch()

    def keys(self) -> Iterable[Type[Block] | Type[Inline] | Type[Header]]:
        return (
            set(self.header_makers.keys())
            .union(self.block_makers.keys())
            .union(self.inline_makers.keys())
        )

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


PANDOC_FORMAT_TO_EXT = {
    "asciidoc": ".adoc",
    "docbook": ".db",
    "docbook4": ".db",
    "docbook5": ".db",
    "latex": ".tex",
    "commonmark": "md",
    "commonmark_x": "md",
    "markdown": "md",
    "markdown_mmd": "md",
    "markdown_phpextra": "md",
    "markdown_strict": "md",
    "gfm": "md",
    "odt": "odt",
    "docx": "docx",
    "rst": "rst",
}

def recommend_pandoc_format_ext(format: str) -> str:
    if format in PANDOC_FORMAT_TO_EXT:
        return PANDOC_FORMAT_TO_EXT[format]
    return format

class PandocSetup(RenderSetup[PandocRenderer]):
    pandoc_format: str

    meta: pan.Meta
    makers: PandocDispatch[PandocRenderer]
    counter_rendering: Dict[str, Optional[SimpleCounterFormat[SimpleCounterStyle]]]
    """Counters can either be renderable or not - unrenderable counters are controlled by Pandoc e.g. footnotes, section headings and must always be backreferenced natively."""
    requested_counter_links: List[CounterLink]
    counters: CounterState
    pandoc_options: List[str]
    """Options to be passed to pandoc on the command line"""

    def __init__(self, pandoc_format: str) -> None:
        super().__init__()
        self.pandoc_format = pandoc_format

        self.meta = pan.Meta({})
        self.makers = PandocRenderer.default_makers()
        self.counter_rendering = {}
        self.requested_counter_links = []
        self.pandoc_options = []

    def register_plugins(
        self, build_sys: BuildSystem, plugins: Iterable[RenderPlugin["PandocSetup"]]
    ) -> None:
        super().register_plugins(build_sys, plugins)
        for parent, counter in self.requested_counter_links:
            if parent:
                if (self.counter_rendering[parent] is None) and (
                    self.counter_rendering[counter] is not None
                ):
                    raise ValueError(
                        f"Can't have a link between parent counter '{parent}' and child '{counter}'.\n\
                        '{parent} is Pandoc-controlled and not renderable.\n\
                        '{counter}' is turnip_text-controlled or pandoc-consistent, and must be rendered, but that would require using the parent."
                    )

        # Now we know the full hierarchy we can build the CounterState
        self.counters = CounterState(
            build_counter_hierarchy(
                self.requested_counter_links, set(self.counter_rendering.keys())
            ),
        )

    def gen_dfs_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]]:
        vs: List[Tuple[VisitorFilter, VisitorFunc]] = [
            (None, self.counters.count_anchor_if_present)
        ]
        for p in self.plugins:
            v = p._make_visitors()
            if v:
                vs.extend(v)
        return vs

    def known_node_types(
        self,
    ) -> Iterable[type[Block] | type[Inline] | type[Header]]:
        return self.makers.keys()

    def known_countables(self) -> Iterable[str]:
        return self.counters.anchor_kind_to_parent_chain.keys()

    def define_renderable_counter(
        self,
        counter: str,
        counter_format: SimpleCounterFormat[SimpleCounterStyle],
    ) -> None:
        """
        Given a counter which pandoc handles in a consistent way to turnip_text,
        define how turnip_text should number it and render backreferences to it.
        """
        if counter not in self.counter_rendering:
            self.counter_rendering[counter] = counter_format

    def define_unrenderable_counter(
        self,
        counter: str,
    ) -> None:
        """
        Note that a given counter is pandoc-controlled and not consistent with turnip_text,
        and thus cannot be rendered.
        """
        if counter not in self.counter_rendering:
            self.counter_rendering[counter] = None

    def request_counter_parent(
        self, counter: str, parent_counter: Optional[str]
    ) -> None:
        assert (
            counter in self.counter_rendering
        ), "Counter must be defined before you request parentage"
        if parent_counter:
            assert (
                parent_counter in self.counter_rendering
            ), "Parent counter must be defined before you request parentage"
        # Apply the requested counter links
        self.requested_counter_links.append((parent_counter, counter))

    def add_pandoc_options(self, *args: str) -> None:
        self.pandoc_options.extend(args)

    def render_document(
        self,
        fmt: FmtEnv,
        anchors: StdAnchorPlugin,
        document: Document,
        build_sys: BuildSystem,
        output_file_name: Optional[OutputRelPath],
    ) -> None:
        default_output_file_name = "document.docx"
        output_file = build_sys.resolve_output_file(
            RelPath(output_file_name or default_output_file_name)
        )

        renderer = PandocRenderer(
            fmt,
            anchors,
            self.meta,
            self.makers,
            self.counters,
            self.counter_rendering,
            self.pandoc_options,
        )
        pan_doc = renderer.make_document(document)

        # Do this early in case the files are for the final pandoc run
        build_sys.run_deferred_jobs()

        output_path = str(output_file.external_path)
        pandoc.write(
            pan_doc,
            file=output_path,
            format=pandoc.format_from_filename(output_path),
            options=renderer.pandoc_options,
        )


PandocPlugin = RenderPlugin[PandocSetup]
