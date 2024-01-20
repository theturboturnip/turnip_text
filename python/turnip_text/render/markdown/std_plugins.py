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
    FootnoteContents,
    FootnoteRef,
    InlineFormatted,
    InlineFormattingType,
    NamedUrl,
    StructureBlockHeader,
)
from turnip_text.helpers import paragraph_of
from turnip_text.render import (
    EmitterDispatch,
    RefEmitterDispatch,
    RenderPlugin,
    VisitorFilter,
    VisitorFunc,
)
from turnip_text.render.counters import (
    CounterChainValue,
    CounterLink,
    CounterState,
    build_counter_hierarchy,
)
from turnip_text.render.markdown.renderer import MarkdownRenderer


def STD_MARKDOWN_RENDER_PLUGINS(
    use_chapters: bool,
    indent_list_items: bool = True,
    requested_counter_links: Optional[Dict[Optional[str], str]] = None,
) -> List[RenderPlugin[MarkdownRenderer]]:
    return [
        StructureRenderPlugin(use_chapters),
        UncheckedBibMarkdownRenderPlugin(),
        FootnoteAtEndRenderPlugin(),
        ListRenderPlugin(indent_list_items),
        InlineFormatRenderPlugin(),
        UrlRenderPlugin(),
    ]


class StructureRenderPlugin(RenderPlugin[MarkdownRenderer]):
    _has_chapter: bool

    def __init__(self, use_chapters: bool) -> None:
        super().__init__()
        self._has_chapter = use_chapters

    def _register_node_handlers(
        self, handlers: EmitterDispatch[MarkdownRenderer]
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
                    renderer.emit(head.anchor, renderer.anchor_to_number_text(head.anchor), " ")
                renderer.emit(head.contents)

            renderer.emit_break_paragraph()
            renderer.emit_blockscope(contents)
        else:
            renderer.emit_raw("#" * (head.weight) + " ")
            if head.anchor:
                renderer.emit(head.anchor, renderer.anchor_to_number_text(head.anchor), " ")
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
        self, handlers: EmitterDispatch[MarkdownRenderer]
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
            DocSegment(
                doc.heading1() @ paragraph_of("Footnotes"),
                BlockScope([FootnoteList()]),
                [],
            )
        )
        return toplevel

    def _register_node_handlers(
        self, handlers: EmitterDispatch[MarkdownRenderer]
    ) -> None:
        handlers.register_block_or_inline(FootnoteRef, self._emit_footnote_ref)
        handlers.register_block_or_inline(FootnoteContents, lambda _, __, ___: None)
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
        renderer.emit(footnote.portal_to)

    def _emit_footnotes(
        self,
        footnotes: FootnoteList,
        renderer: MarkdownRenderer,
        ctx: FormatContext,
    ) -> None:
        for i, backref in enumerate(self.footnote_anchors):
            anchor, footnote = renderer.anchors.lookup_backref_float(backref)
            assert isinstance(footnote, FootnoteContents)
            renderer.emit(anchor, f"^{i}: ", footnote.contents)
            renderer.emit_break_sentence()
        renderer.emit_break_paragraph()


class ListRenderPlugin(RenderPlugin[MarkdownRenderer]):
    indent_list_items: bool = True

    def __init__(self, indent_list_items: bool = True):
        self.indent_list_items = indent_list_items

    def _register_node_handlers(
        self, handlers: EmitterDispatch[MarkdownRenderer]
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
        self, handlers: EmitterDispatch[MarkdownRenderer]
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
        self, handlers: EmitterDispatch[MarkdownRenderer]
    ) -> None:
        handlers.register_block_or_inline(NamedUrl, self._emit_url)

    def _register_ref_handlers(
        self, handlers: RefEmitterDispatch[MarkdownRenderer]
    ) -> None:
        handlers.register_anchor_render_method(
            "url", self._emit_anchor_url, self._emit_backref_url, can_be_default=True
        )

    def _emit_anchor_url(
        self,
        renderer: MarkdownRenderer,
        fmt: FormatContext,
        anchor: Anchor,
    ):
        renderer.emit_empty_tag("a", f'id="{anchor.canonical()}"')

    def _emit_backref_url(
        self,
        renderer: MarkdownRenderer,
        fmt: FormatContext,
        backref: Backref,
    ):
        anchor = renderer.anchors.lookup_backref(backref)
        if backref.label_contents:
            renderer.emit(fmt.url(f"#{anchor.canonical()}") @ backref.label_contents)
        else:
            renderer.emit(
                fmt.url(f"#{anchor.canonical()}") @ renderer.anchor_to_ref_text(anchor)
            )

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
