from typing import Any, Dict, Iterable, Iterator, List, Optional, Tuple, cast

from turnip_text import Block, BlockScope, DocSegment, Inline
from turnip_text.doc import FormatContext
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
from turnip_text.render import (
    EmitterDispatch,
    RefEmitterDispatch,
    RenderPlugin,
    VisitorFilter,
    VisitorFunc,
)
from turnip_text.render.counters import (
    CounterChainValue,
    CounterHierarchy,
    CounterLink,
    CounterState,
    build_counter_hierarchy,
)
from turnip_text.render.latex.renderer import LatexRenderer


def STD_LATEX_RENDER_PLUGINS(
    use_chapters: bool,
    indent_list_items: bool = True,
    requested_counter_links: Optional[Dict[Optional[str], str]] = None,
) -> List[RenderPlugin[LatexRenderer]]:
    return [
        StructureRenderPlugin(use_chapters),
        UncheckedBiblatexRenderPlugin(),
        FootnoteRenderPlugin(),
        ListRenderPlugin(indent_list_items),
        InlineFormatRenderPlugin(),
        UrlRenderPlugin(),
        CleverefBackrefPlugin(),
    ]


class StructureRenderPlugin(RenderPlugin[LatexRenderer]):
    level_to_latex: List[Optional[str]]

    def __init__(self, use_chapters: bool) -> None:
        super().__init__()
        if use_chapters:
            self.level_to_latex = [
                None,
                "chapter",
                "section",
                "subsection",
                "subsubsection",
            ]
        else:
            self.level_to_latex = [None, "section", "subsection", "subsubsection"]

    def _register_node_handlers(self, handlers: EmitterDispatch[LatexRenderer]) -> None:
        handlers.register_header(StructureBlockHeader, self._emit_structure)

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
        renderer: LatexRenderer,
        ctx: FormatContext,
    ) -> None:
        latex_name = self.level_to_latex[head.weight]
        if latex_name is None:
            raise ValueError(
                f"Can't emit {head} because it uses an unusable weight: {head.weight}"
            )
        if head.anchor:
            # This is a numbered entry with a label
            renderer.emit_macro(latex_name)  # i.e. r"\section"
        else:
            renderer.emit_macro(latex_name + "*")
        renderer.emit_braced(head.contents)  # i.e. r"\section*" + "{Section Name}"
        if head.anchor:
            renderer.emit(
                head.anchor
            )  # i.e. r"\section*{Section Name}\label{h1:Section_Name}"
        renderer.emit_break_paragraph()
        # Now emit the rest of the damn doc :)
        renderer.emit_blockscope(contents)
        for s in subsegments:
            renderer.emit_segment(s)


class UncheckedBiblatexRenderPlugin(RenderPlugin[LatexRenderer]):
    def _register_node_handlers(self, handlers: EmitterDispatch[LatexRenderer]) -> None:
        handlers.register_block_or_inline(Citation, self._emit_cite)
        handlers.register_block_or_inline(CiteAuthor, self._emit_citeauthor)
        handlers.register_block_or_inline(Bibliography, self._emit_bibliography)

    def _emit_cite(
        self, cite: Citation, renderer: LatexRenderer, ctx: FormatContext
    ) -> None:
        renderer.emit_macro("cite")
        if cite.contents:
            renderer.emit_sqr_bracketed(cite.contents)
        renderer.emit_braced(",".join(cite.citekeys))

    def _emit_citeauthor(
        self,
        citeauthor: CiteAuthor,
        renderer: LatexRenderer,
        ctx: FormatContext,
    ) -> None:
        renderer.emit_macro("citeauthor")
        renderer.emit_braced(citeauthor.citekey)

    def _emit_bibliography(
        self,
        bib: Bibliography,
        renderer: LatexRenderer,
        ctx: FormatContext,
    ) -> None:
        renderer.emit("{")
        renderer.emit_break_sentence()
        with renderer.indent(4):
            renderer.emit("\\raggedright")
            renderer.emit_break_sentence()
            renderer.emit("\\printbibliography[heading=none]")
            renderer.emit_break_sentence()
        renderer.emit("}")
        renderer.emit_break_paragraph()


class FootnoteRenderPlugin(RenderPlugin[LatexRenderer]):
    def _register_node_handlers(self, handlers: EmitterDispatch[LatexRenderer]) -> None:
        handlers.register_block_or_inline(FootnoteRef, self._emit_footnote)
        handlers.register_block_or_inline(FootnoteContents, lambda _, __, ___: None)

    def _requested_counters(self) -> Iterable[CounterLink]:
        return ((None, "footnote"),)

    def _emit_footnote(
        self,
        footnote: FootnoteRef,
        renderer: LatexRenderer,
        ctx: FormatContext,
    ) -> None:
        footnote_backref = footnote.portal_to
        _, footnote_contents = renderer.anchors.lookup_backref_float(footnote_backref)
        if footnote_contents is None:
            raise ValueError(f"Reference to nonexistant footnote {footnote_backref}")
        assert isinstance(footnote_contents, FootnoteContents)
        renderer.emit_macro("footnote")
        renderer.emit_braced(footnote_contents.contents)


class ListRenderPlugin(RenderPlugin[LatexRenderer]):
    indent_list_items: bool = True

    def __init__(self, indent_list_items: bool = True):
        self.indent_list_items = indent_list_items

    def _register_node_handlers(self, handlers: EmitterDispatch[LatexRenderer]) -> None:
        handlers.register_block_or_inline(DisplayList, self._emit_list)
        handlers.register_block_or_inline(DisplayListItem, self._emit_list_item)

    def _emit_list(
        self,
        list: DisplayList,
        renderer: LatexRenderer,
        ctx: FormatContext,
    ) -> None:
        mode = {
            DisplayListType.Itemize: "itemize",
            DisplayListType.Enumerate: "enumerate",
        }[list.list_type]
        with renderer.emit_env(mode):
            renderer.emit(*list.contents, joiner=renderer.emit_break_paragraph)

    def _emit_list_item(
        self,
        list_item: DisplayListItem,
        renderer: LatexRenderer,
        ctx: FormatContext,
    ) -> None:
        renderer.emit_macro("item")
        # Put {} after \item so square brackets at the start of render_block don't get swallowed as arguments
        renderer.emit("{} ")
        indent_width = len("\\item{} ")
        with renderer.indent(indent_width):
            renderer.emit(list_item.contents)


FORMAT_TYPE_TO_MACRO = {
    InlineFormattingType.Bold: "textbf",
    InlineFormattingType.Italic: "textit",
    InlineFormattingType.Underline: "underline",
    InlineFormattingType.Emph: "emph",
    InlineFormattingType.Strong: "strong",
}


class InlineFormatRenderPlugin(RenderPlugin[LatexRenderer]):
    # TODO enquote/csquotes package?
    def _register_node_handlers(self, handlers: EmitterDispatch[LatexRenderer]) -> None:
        handlers.register_block_or_inline(InlineFormatted, self._emit_formatted)

    def _emit_formatted(
        self,
        f: InlineFormatted,
        renderer: LatexRenderer,
        fmt: FormatContext,
    ) -> None:
        if f.format_type == InlineFormattingType.SingleQuote:
            renderer.emit("`", f.contents, "'")
        elif f.format_type == InlineFormattingType.DoubleQuote:
            renderer.emit("``", f.contents, "''")
        else:
            # All other kinds are just the contents wrapped in a macro
            renderer.emit_macro(FORMAT_TYPE_TO_MACRO[f.format_type])
            renderer.emit_braced(f.contents)


class UrlRenderPlugin(RenderPlugin[LatexRenderer]):
    # TODO add dependency on hyperref!!
    def _register_node_handlers(self, handlers: EmitterDispatch[LatexRenderer]) -> None:
        handlers.register_block_or_inline(NamedUrl, self._emit_url)

    def _emit_url(
        self,
        url: NamedUrl,
        renderer: LatexRenderer,
        fmt: FormatContext,
    ) -> None:
        if "}" in url.url:
            raise RuntimeError(
                f"Can't handle url {url.url} with a }} in it. Please use proper percent-encoding to escape it."
            )

        # TODO this breaks if the hash is already escaped :|

        if url.contents is None:
            renderer.emit_macro("url")
            renderer.emit_braced(url.url.replace("#", "\\#"))
        else:
            renderer.emit_macro("href")
            renderer.emit_braced(url.url.replace("#", "\\#"))
            renderer.emit_braced(url.contents)


class CleverefBackrefPlugin(RenderPlugin[LatexRenderer]):
    # TODO add dependency on cleveref?

    def _register_node_handlers(self, handlers: EmitterDispatch[LatexRenderer]) -> None:
        return None

    def _register_ref_handlers(
        self, handlers: RefEmitterDispatch[LatexRenderer]
    ) -> None:
        handlers.register_anchor_render_method(
            "cleveref",
            self._emit_anchor_cleveref,
            self._emit_backref_cleveref,
            can_be_default=True,
        )

    def _emit_backref_cleveref(
        self,
        renderer: LatexRenderer,
        fmt: FormatContext,
        backref: Backref,
    ):
        # TODO if the backref has label_contents, respect that
        renderer.emit_macro("cref")
        renderer.emit_braced(renderer.anchors.lookup_backref(backref).canonical())

    def _emit_anchor_cleveref(
        self,
        renderer: LatexRenderer,
        fmt: FormatContext,
        anchor: Anchor,
    ):
        # TODO this isn't what we want at all!!! If the anchor is not directly next to the counter of its kind, cleveref will pick up the wrong type
        if anchor.id:
            renderer.emit_macro("label")
            renderer.emit_braced(anchor.canonical())
