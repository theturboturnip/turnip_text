from typing import (
    Any,
    Dict,
    Generator,
    Iterable,
    Iterator,
    List,
    Optional,
    Sequence,
    Set,
    Tuple,
    Type,
    Union,
)

from turnip_text import (
    Block,
    BlockScope,
    DocSegment,
    DocSegmentHeader,
    Inline,
    UnescapedText,
)
from turnip_text.doc import DocState, FormatContext
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.doc.std_plugins import (
    Bibliography,
    Citation,
    CiteAuthor,
    DisplayList,
    DisplayListItem,
    DisplayListType,
    FootnoteRef,
    InlineFormatted,
    InlineFormattingType,
    NamedUrl,
    StructureBlockHeader,
)
from turnip_text.render import (
    RendererHandlers,
    RenderPlugin,
    VisitorFilter,
    VisitorFunc,
)
from turnip_text.render.counters import (
    CounterChainValue,
    CounterLink,
    CounterSet,
    build_counter_hierarchy,
)
from turnip_text.render.markdown.renderer import MarkdownRenderer


def STD_MARKDOWN_RENDER_PLUGINS(
    use_chapters: bool,
    indent_list_items: bool = True,
    requested_counter_links: Optional[Dict[Optional[str], str]] = None,
) -> List[RenderPlugin[MarkdownRenderer]]:
    ps = [
        StructureRenderPlugin(use_chapters),
        UncheckedBibMarkdownRenderPlugin(),
        FootnoteAtEndRenderPlugin(),
        ListRenderPlugin(indent_list_items),
        InlineFormatRenderPlugin(),
        UrlRenderPlugin(),
    ]
    anchors = AnchorCountingBackrefPlugin(
        [(k, v) for k, v in requested_counter_links.items()]
        if requested_counter_links
        else list(),
        ps,
    )
    ps.append(anchors)
    return ps


class StructureRenderPlugin(RenderPlugin[MarkdownRenderer]):
    _has_chapter: bool

    def __init__(self, use_chapters: bool) -> None:
        super().__init__()
        self._has_chapter = use_chapters

    def _register_node_handlers(
        self, handlers: RendererHandlers[MarkdownRenderer]
    ) -> None:
        handlers.register_header(StructureBlockHeader, self._emit_structure)

    # TODO register name generators for counters based on _has_chapter
    # if has_chapter, weight=1 => chapter, weight=2 => section
    # else weight=1 => section

    def _requested_counters(self) -> Iterable[CounterLink]:
        return (
            (None, "h1"),
            ("h1", "h2"),
            ("h2", "h3"),
            ("h3", "h4"),
        )

    def _emit_structure(
        self,
        head: StructureBlockHeader,
        contents: BlockScope,
        subsegments: Iterator[DocSegment],
        renderer: MarkdownRenderer,
        ctx: FormatContext,
    ) -> None:
        if renderer.in_html_mode:
            tag = f"h{head.weight}"

            with renderer.emit_tag(tag):
                if head.anchor:
                    raise NotImplementedError("Generate prefix for structure")
                    renderer.emit(head.anchor, ".".join(str(n) for n in head.num), " ")
                renderer.emit(head.contents)

            renderer.emit_break_paragraph()
            renderer.emit_blockscope(contents)
        else:
            renderer.emit_raw("#" * (head.weight) + " ")
            if head.anchor:
                raise NotImplementedError("Generate prefix for structure")
                renderer.emit(head.anchor, ".".join(str(n) for n in head.num), " ")
            renderer.emit(head.contents)
            renderer.emit_break_paragraph()
            renderer.emit_blockscope(contents)


# TODO footnotes and citations
# Footnotes may require changes to document structure (e.g. a FootnoteFlush block after a paragraph with a footnote in it?)
# How to handle this?
class UncheckedBibMarkdownRenderPlugin(RenderPlugin[MarkdownRenderer]):
    _ordered_citations: List[str]
    _referenced_citations: Set[str]

    def __init__(self) -> None:
        super().__init__()
        self._ordered_citations = []
        self._referenced_citations = set()

    def _register_node_handlers(
        self, handlers: RendererHandlers[MarkdownRenderer]
    ) -> None:
        handlers.register_block_or_inline(Citation, self._emit_cite)
        handlers.register_block_or_inline(CiteAuthor, self._emit_citeauthor)
        handlers.register_block_or_inline(Bibliography, self._emit_bibliography)

    def _register_citation(self, citekey: str) -> None:
        if citekey not in self._referenced_citations:
            self._referenced_citations.add(citekey)
            self._ordered_citations.append(citekey)

    def _make_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]] | None:
        def regsiter_many_citations(c: Citation):
            for k in c.citekeys:
                self._register_citation(k)

        return [
            (Citation, regsiter_many_citations),
            (CiteAuthor, lambda ca: self._register_citation(ca.citekey)),
        ]

    # TODO make Citations use backrefs?

    def _emit_cite(
        self, cite: Citation, renderer: MarkdownRenderer, ctx: FormatContext
    ) -> None:
        # TODO what happens with unmarkdownable labels? e.g. labels with backslash or something. need to check that when loading.
        # TODO also maybe people wouldn't want those labels being exposed?

        if cite.contents:
            renderer.emit(UnescapedText("("))
        for citekey in cite.citekeys:
            renderer.emit(ctx.url(f"#{citekey}") @ f"[{citekey}]")
        if cite.contents:
            renderer.emit(cite.contents, UnescapedText(", )"))

    def _emit_citeauthor(
        self,
        citeauthor: CiteAuthor,
        renderer: MarkdownRenderer,
        ctx: FormatContext,
    ) -> None:
        renderer.emit(UnescapedText("The authors of "))
        renderer.emit(ctx.url(f"#{citeauthor.citekey}") @ f"[{citeauthor.citekey}]")

    def _emit_bibliography(
        self,
        bib: Bibliography,
        renderer: MarkdownRenderer,
        ctx: FormatContext,
    ) -> None:
        # TODO actual reference rendering!
        def bib_gen() -> Generator[None, None, None]:
            for citekey in self._referenced_citations:
                renderer.emit_empty_tag("a", f'id="{citekey}"')
                renderer.emit(
                    UnescapedText(
                        f"[{citekey}]: TODO make citation text for {citekey}"
                    ),
                )
                yield

        renderer.emit_join_gen(bib_gen(), renderer.emit_break_paragraph)


class FootnoteList(Block):
    pass


class FootnoteAtEndRenderPlugin(RenderPlugin[MarkdownRenderer]):
    footnote_anchors: List[Backref]

    def __init__(self) -> None:
        super().__init__()
        self.footnote_anchors = []

    def _doc_nodes(
        self,
    ) -> Sequence[type[Block] | type[Inline] | type[DocSegmentHeader]]:
        return [FootnoteList]

    def _mutate_document(
        self, doc: DocState, fmt: FormatContext, toplevel: DocSegment
    ) -> DocSegment:
        toplevel.push_subsegment(
            DocSegment(doc.heading1() @ ["Footnotes"], BlockScope([FootnoteList()]), [])
        )
        return toplevel

    def _register_node_handlers(
        self, handlers: RendererHandlers[MarkdownRenderer]
    ) -> None:
        handlers.register_block_or_inline(FootnoteRef, self._emit_footnote_ref)
        handlers.register_block_or_inline(FootnoteList, self._emit_footnotes)

    def _requested_counters(self) -> Iterable[CounterLink]:
        return ((None, "footnote"),)

    def _make_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]] | None:
        return [(FootnoteRef, lambda f: self.footnote_anchors.append(f.backref))]

    def _emit_footnote_ref(
        self,
        footnote: FootnoteRef,
        renderer: MarkdownRenderer,
        ctx: FormatContext,
    ) -> None:
        # TODO hook into the anchor rendering and register a handler for footnotes
        renderer.emit(footnote.backref)

    def _emit_footnotes(
        self,
        footnotes: FootnoteList,
        renderer: MarkdownRenderer,
        ctx: FormatContext,
    ) -> None:
        for i, backref in enumerate(self.footnote_anchors):
            renderer.emit(
                renderer.doc.anchors.lookup_backref(backref),
                f"^{i}: ",
                renderer.doc.lookup_float_from_backref(backref),
            )
            renderer.emit_break_sentence()
        renderer.emit_break_paragraph()


class ListRenderPlugin(RenderPlugin[MarkdownRenderer]):
    indent_list_items: bool = True

    def __init__(self, indent_list_items: bool = True):
        self.indent_list_items = indent_list_items

    def _register_node_handlers(
        self, handlers: RendererHandlers[MarkdownRenderer]
    ) -> None:
        handlers.register_block_or_inline(DisplayList, self._emit_list)
        handlers.register_block_or_inline(DisplayListItem, self._emit_list_item)

    def _emit_list_item(
        self,
        list_item: DisplayListItem,
        renderer: MarkdownRenderer,
        ctx: FormatContext,
    ) -> None:
        pass  # DisplayListItems inside DisplayLists will be handled directly

    def _emit_list(
        self,
        list: DisplayList,
        renderer: MarkdownRenderer,
        ctx: FormatContext,
    ) -> None:
        numbered = list.list_type == DisplayListType.Enumerate
        if renderer.in_html_mode:

            def emit_elem() -> Generator[None, None, None]:
                for item in list.contents:
                    renderer.emit_raw("<li>")
                    renderer.emit_newline()
                    with renderer.indent(4):
                        if isinstance(item, DisplayList):
                            renderer.emit_block(item)
                        else:
                            renderer.emit_blockscope(item.contents)
                    renderer.emit_newline()
                    renderer.emit_raw("</li>")
                    yield None

            tag = "ol" if numbered else "ul"
            renderer.emit_raw(f"<{tag}>")
            with renderer.indent(4):
                renderer.emit_break_sentence()
                renderer.emit_join_gen(emit_elem(), renderer.emit_break_sentence)
            renderer.emit_raw(f"</{tag}>")
        else:
            if numbered:

                def emit_numbered() -> Generator[None, None, None]:
                    for idx, item in enumerate(list.contents):
                        indent = f"{idx+1}. "
                        renderer.emit_raw(indent)
                        with renderer.indent(len(indent)):
                            if isinstance(item, DisplayList):
                                renderer.emit_block(item)
                            else:
                                renderer.emit_blockscope(item.contents)
                        yield None

                renderer.emit_join_gen(emit_numbered(), renderer.emit_break_sentence)
            else:

                def emit_dashed() -> Generator[None, None, None]:
                    for idx, item in enumerate(list.contents):
                        indent = f"- "
                        renderer.emit_raw(indent)
                        with renderer.indent(len(indent)):
                            if isinstance(item, DisplayList):
                                renderer.emit_block(item)
                            else:
                                renderer.emit_blockscope(item.contents)
                        yield None

                renderer.emit_join_gen(emit_dashed(), renderer.emit_break_sentence)


FORMAT_TYPE_TO_MARKDOWN = {
    InlineFormattingType.Bold: "**",
    InlineFormattingType.Italic: "*",
    InlineFormattingType.Emph: "*",  # = italic
    InlineFormattingType.Strong: "**",  # = bold
}

FORMAT_TYPE_TO_HTML = {
    InlineFormattingType.Bold: "b",
    InlineFormattingType.Italic: "i",
    InlineFormattingType.Underline: "u",
    InlineFormattingType.Emph: "em",
    InlineFormattingType.Strong: "strong",
}


class InlineFormatRenderPlugin(RenderPlugin[MarkdownRenderer]):
    def _register_node_handlers(
        self, handlers: RendererHandlers[MarkdownRenderer]
    ) -> None:
        handlers.register_block_or_inline(InlineFormatted, self._emit_formatted)

    def _emit_formatted(
        self,
        f: InlineFormatted,
        renderer: MarkdownRenderer,
        fmt: FormatContext,
    ) -> None:
        if f.format_type == InlineFormattingType.SingleQuote:
            renderer.emit_raw("'")
            renderer.emit(f.contents)
            renderer.emit_raw("'")
        elif f.format_type == InlineFormattingType.DoubleQuote:
            renderer.emit_raw('"')
            renderer.emit(f.contents)
            renderer.emit_raw('"')
        elif renderer.in_html_mode:
            with renderer.emit_tag(FORMAT_TYPE_TO_HTML[f.format_type]):
                renderer.emit(f.contents)
        elif f.format_type == InlineFormattingType.Underline:
            # Have to go into html mode for this
            with renderer.emit_tag("u"):
                renderer.emit(f.contents)
        else:
            surround = FORMAT_TYPE_TO_MARKDOWN[f.format_type]
            renderer.emit_raw(surround)
            renderer.emit(f.contents)
            renderer.emit_raw(surround)


class UrlRenderPlugin(RenderPlugin[MarkdownRenderer]):
    # TODO add dependency on hyperref!!
    def _register_node_handlers(
        self, handlers: RendererHandlers[MarkdownRenderer]
    ) -> None:
        handlers.register_block_or_inline(NamedUrl, self._emit_url)

    def _emit_url(
        self,
        url: NamedUrl,
        renderer: MarkdownRenderer,
        fmt: FormatContext,
    ) -> None:
        if "<" in url.url or ">" in url.url or ")" in url.url:
            raise RuntimeError(
                f"Can't handle url {url.url} with a <, >, or ) in it. Please use proper percent-encoding to escape it."
            )

        if renderer.in_html_mode:
            assert ">" not in url.url and "<" not in url.url
            renderer.emit_raw(f'<a href="{url.url}">')
            if url.contents is None:
                # Set the "name" of the URL to the text of the URL - escaped so it can be read as normal markdown
                renderer.emit_unescapedtext(UnescapedText(url.url))
            else:
                renderer.emit(*url.contents)
            renderer.emit_raw("</a>")
        else:
            assert ")" not in url.url
            renderer.emit_raw("[")
            if url.contents is None:
                # Set the "name" of the URL to the text of the URL - escaped so it can be read as normal markdown
                renderer.emit_unescapedtext(UnescapedText(url.url))
            else:
                renderer.emit(*url.contents)
            renderer.emit_raw(f"]({url.url})")


# TODO could do something here with document.counted_anchor_kinds and comparing against supported anchors...
class AnchorCountingBackrefPlugin(RenderPlugin[MarkdownRenderer]):
    node_counters: Dict[
        int, CounterChainValue
    ]  # Mapping of <node id> -> <counter value for node>
    anchor_counters: Dict[
        Tuple[str, str], CounterChainValue
    ]  # Mapping of (kind, id) for <counter value>

    def __init__(
        self,
        counter_links: List[CounterLink],
        other_plugins: List[RenderPlugin[MarkdownRenderer]],
    ) -> None:
        super().__init__()

        for p in other_plugins:
            counter_links.extend(p._requested_counters())

        self.counters = CounterSet(build_counter_hierarchy(counter_links))
        self.node_counters = {}
        self.anchor_counters = {}

    def _register_node_handlers(
        self, handlers: RendererHandlers[MarkdownRenderer]
    ) -> None:
        handlers.register_block_or_inline(Backref, self._emit_backref)
        handlers.register_block_or_inline(Anchor, self._emit_anchor)

    def _make_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]] | None:
        return [(None, self._visit_anchorable)]

    def _visit_anchorable(self, node: Any) -> None:
        # Counter pass

        anchor = getattr(node, "anchor", None)
        if isinstance(anchor, Anchor):
            if anchor.kind not in self.counters.anchor_kind_to_parent_chain:
                raise ValueError(f"Unknown counter kind '{anchor.kind}'")
            # non-None anchors always increment the count, but if anchor.id is None we don't care
            count = self.counters.increment_counter(anchor.kind)
            if anchor.id is not None:
                self.anchor_counters[(anchor.kind, anchor.id)] = count
            self.node_counters[id(node)] = count

    # TODO if anyone needs this, implement it
    # TODO lol no-one can access this dumbass
    def lookup_anchorable_name(self, node: Any) -> Inline:
        raise NotImplementedError()

    def _emit_backref(
        self,
        backref: Backref,
        renderer: MarkdownRenderer,
        fmt: FormatContext,
    ):
        canonical_id = renderer.doc.anchors.lookup_backref(backref).canonical()
        if backref.label_contents:
            renderer.emit(fmt.url(f"#{canonical_id}") @ backref.label_contents)
        else:
            raise NotImplementedError("Backref to inline")

    def _emit_anchor(
        self,
        anchor: Anchor,
        renderer: MarkdownRenderer,
        fmt: FormatContext,
    ):
        if anchor.id:
            renderer.emit_empty_tag("a", f'id="{anchor.canonical()}"')

    def get_anchor_counter(self, a: Anchor) -> Optional[CounterChainValue]:
        if a.id is None:
            return None
        return self.anchor_counters.get((a.kind, a.id), None)

    def get_node_counter(self, n: Any) -> Optional[CounterChainValue]:
        return self.node_counters.get(id(n), None)
