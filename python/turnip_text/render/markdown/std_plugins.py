from typing import Generator, Iterator, List, Sequence, Set, Tuple

from turnip_text import (
    Block,
    BlockScope,
    DocSegment,
    Document,
    Header,
    Inline,
    InlineScope,
    Raw,
    Text,
)
from turnip_text.doc.anchors import Backref
from turnip_text.doc.dfs import VisitorFilter, VisitorFunc
from turnip_text.doc.std_plugins import (
    Bibliography,
    Citation,
    CitationEnvPlugin,
    CiteAuthor,
    DisplayList,
    DisplayListItem,
    DisplayListType,
    FootnoteContents,
    FootnoteEnvPlugin,
    FootnoteRef,
    InlineFormatEnvPlugin,
    InlineFormatted,
    InlineFormattingType,
    ListEnvPlugin,
    NamedUrl,
    StructureEnvPlugin,
    StructureHeader,
    SubfileEnvPlugin,
    UrlEnvPlugin,
)
from turnip_text.env_plugins import DocEnv, FmtEnv
from turnip_text.render.manual_numbering import SimpleCounterFormat
from turnip_text.render.markdown.renderer import (
    MarkdownCounterStyle,
    MarkdownPlugin,
    MarkdownRenderer,
    MarkdownSetup,
)


def STD_MARKDOWN_RENDER_PLUGINS(
    use_chapters: bool,
    indent_list_items: bool = True,
) -> List[MarkdownPlugin]:
    return [
        MarkdownStructurePlugin(use_chapters),
        MarkdownCitationPlugin_UncheckedBib(),
        MarkdownFootnotePlugin_AtEnd(),
        MarkdownListPlugin(indent_list_items),
        MarkdownInlineFormatPlugin(),
        MarkdownUrlPlugin(),
        MarkdownSubfilePlugin(),
    ]


class MarkdownStructurePlugin(MarkdownPlugin, StructureEnvPlugin):
    _has_chapter: bool

    def __init__(self, use_chapters: bool) -> None:
        super().__init__()
        self._has_chapter = use_chapters

    def _register(self, setup: MarkdownSetup) -> None:
        setup.emitter.register_header(StructureHeader, self._emit_structure)
        setup.define_counter_rendering(
            "h1",
            SimpleCounterFormat(
                name=("chapter" if self._has_chapter else "section"),
                style=MarkdownCounterStyle.Arabic,
            ),
        )
        setup.define_counter_rendering(
            "h2",
            SimpleCounterFormat(
                name=("section" if self._has_chapter else "subsection"),
                style=MarkdownCounterStyle.Arabic,
            ),
        )
        setup.define_counter_rendering(
            "h3",
            SimpleCounterFormat(
                name=("subsection" if self._has_chapter else "subsubsection"),
                style=MarkdownCounterStyle.Arabic,
            ),
        )
        setup.define_counter_rendering(
            "h4",
            SimpleCounterFormat(
                name=("subsubsection" if self._has_chapter else "subsubsubsection"),
                style=MarkdownCounterStyle.Arabic,
            ),
        )
        setup.request_counter_parent("h1", parent_counter=None)
        setup.request_counter_parent("h2", parent_counter="h1")
        setup.request_counter_parent("h3", parent_counter="h2")
        setup.request_counter_parent("h4", parent_counter="h3")

    def _emit_structure(
        self,
        head: StructureHeader,
        contents: BlockScope,
        subsegments: Iterator[DocSegment],
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        if renderer.in_html_mode:
            tag = f"h{head.weight}"

            with renderer.emit_tag(tag):
                if head.anchor:
                    renderer.emit(
                        head.anchor,
                        renderer.anchor_to_number_text(head.anchor),
                        Raw(" "),
                    )
                renderer.emit(head.title)
        else:
            renderer.emit_raw("#" * (head.weight) + " ")
            if head.anchor:
                renderer.emit(
                    head.anchor,
                    renderer.anchor_to_number_text(head.anchor),
                    Raw(" "),
                )
            renderer.emit(head.title)

        renderer.emit_break_paragraph()
        renderer.emit_blockscope(contents)
        for s in subsegments:
            renderer.emit_segment(s)


class MarkdownCitationPlugin_UncheckedBib(MarkdownPlugin, CitationEnvPlugin):
    _ordered_citations: List[str]
    _referenced_citations: Set[str]

    def __init__(self) -> None:
        super().__init__()
        self._ordered_citations = []
        self._referenced_citations = set()

    def _register(self, setup: MarkdownSetup) -> None:
        setup.emitter.register_block_or_inline(Citation, self._emit_cite)
        setup.emitter.register_block_or_inline(CiteAuthor, self._emit_citeauthor)
        setup.emitter.register_block_or_inline(Bibliography, self._emit_bibliography)

    def _register_citation(self, citekey: str) -> None:
        if citekey not in self._referenced_citations:
            self._referenced_citations.add(citekey)
            self._ordered_citations.append(citekey)

    def _make_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]] | None:
        def regsiter_many_citations(c: Citation) -> None:
            for k in c.citekeys:
                self._register_citation(k)

        return [
            (Citation, regsiter_many_citations),
            (CiteAuthor, lambda ca: self._register_citation(ca.citekey)),
        ]

    # TODO make Citations use backrefs? Requires document mutations which we don't have yet.

    def _emit_cite(
        self, cite: Citation, renderer: MarkdownRenderer, fmt: FmtEnv
    ) -> None:
        # TODO what happens with unmarkdownable labels? e.g. labels with backslash or something. need to check that when loading.
        # TODO also maybe people wouldn't want those labels being exposed?

        if cite.citenote:
            renderer.emit(Text("("))
        for citekey in cite.citekeys:
            renderer.emit(fmt.url(f"#{citekey}") @ f"[{citekey}]")
        if cite.citenote:
            renderer.emit(cite.citenote, Text(", )"))

    def _emit_citeauthor(
        self,
        citeauthor: CiteAuthor,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        renderer.emit(Text("The authors of "))
        renderer.emit(fmt.url(f"#{citeauthor.citekey}") @ f"[{citeauthor.citekey}]")

    def _emit_bibliography(
        self,
        bib: Bibliography,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        # TODO actual reference rendering!
        def bib_gen() -> Generator[None, None, None]:
            for citekey in self._referenced_citations:
                renderer.emit_empty_tag("a", f'id="{citekey}"')
                renderer.emit(
                    Text(f"[{citekey}]: TODO make citation text for {citekey}"),
                )
                yield

        renderer.emit_join_gen(bib_gen(), renderer.emit_break_paragraph)


class FootnoteList(Block):
    pass


# TODO FootnoteBeforeNextParagraphRenderPlugin
# - FootnoteAfterBlock may try to emit something in the middle of a custom block, Paragraphs are (I think?) guaranteed to be inside a BlockScope and we can kind emit them there
# TODO this is effectively an alternate/nonstandard implementation - move it out, we haven't agreed on a standard footntoe plugin
class MarkdownFootnotePlugin_AtEnd(MarkdownPlugin, FootnoteEnvPlugin):
    footnote_anchors: List[Backref]

    def __init__(self) -> None:
        super().__init__()
        self.footnote_anchors = []

    def _doc_nodes(
        self,
    ) -> Sequence[type[Block] | type[Inline] | type[Header]]:
        return [FootnoteList] + list(super()._doc_nodes())

    def _mutate_document(
        self, doc_env: DocEnv, fmt: FmtEnv, toplevel: Document
    ) -> Document:
        toplevel = super()._mutate_document(doc_env, fmt, toplevel)
        toplevel.push_segment(
            DocSegment(
                doc_env.heading1(num=False) @ "Footnotes",
                BlockScope([FootnoteList()]),
                [],
            )
        )
        return toplevel

    def _register(self, setup: MarkdownSetup) -> None:
        setup.emitter.register_block_or_inline(FootnoteRef, self._emit_footnote_ref)
        setup.emitter.register_block_or_inline(
            FootnoteContents, lambda _, __, ___: None
        )
        setup.emitter.register_block_or_inline(FootnoteList, self._emit_footnotes)
        setup.define_counter_rendering(
            "footnote",
            SimpleCounterFormat(name="^", style=MarkdownCounterStyle.Arabic),
        )

    def _make_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]] | None:
        return [(FootnoteRef, lambda f: self.footnote_anchors.append(f.portal_to))]

    def _emit_footnote_ref(
        self,
        footnote: FootnoteRef,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        renderer.emit(footnote.portal_to)

    def _emit_footnotes(
        self,
        footnotes: FootnoteList,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        for backref in self.footnote_anchors:
            anchor, footnote = renderer.anchors.lookup_backref_float(backref)
            assert isinstance(footnote, FootnoteContents)
            renderer.emit(
                anchor,
                renderer.anchor_to_ref_text(anchor),
                Text(f": "),
                footnote.contents,
            )
            renderer.emit_break_sentence()
        renderer.emit_break_paragraph()


class MarkdownListPlugin(MarkdownPlugin, ListEnvPlugin):
    indent_list_items: bool = True

    def __init__(self, indent_list_items: bool = True):
        self.indent_list_items = indent_list_items

    def _register(self, setup: MarkdownSetup) -> None:
        setup.emitter.register_block_or_inline(DisplayList, self._emit_list)
        setup.emitter.register_block_or_inline(DisplayListItem, self._emit_list_item)

    def _emit_list_item(
        self,
        list_item: DisplayListItem,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        pass  # DisplayListItems inside DisplayLists will be handled directly

    def _emit_list(
        self,
        list: DisplayList,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
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


class MarkdownInlineFormatPlugin(MarkdownPlugin, InlineFormatEnvPlugin):
    def _register(self, setup: MarkdownSetup) -> None:
        setup.emitter.register_block_or_inline(InlineFormatted, self._emit_formatted)

    def _emit_formatted(
        self,
        f: InlineFormatted,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
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


class MarkdownUrlPlugin(MarkdownPlugin, UrlEnvPlugin):
    def _register(self, setup: MarkdownSetup) -> None:
        setup.emitter.register_block_or_inline(NamedUrl, self._emit_url)

    def _emit_url(
        self,
        url: NamedUrl,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        renderer.emit_url(url.url, InlineScope(list(url.name)) if url.name else None)


class MarkdownSubfilePlugin(MarkdownPlugin, SubfileEnvPlugin):
    pass
