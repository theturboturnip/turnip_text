from typing import Any, Dict, Iterable, Iterator, List, Optional, Tuple

from turnip_text import Block, BlockScope, DocSegment, Inline
from turnip_text.doc import FormatContext
from turnip_text.doc.anchors import Anchor, Backref, DocAnchors
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
    CounterHierarchy,
    CounterLink,
    CounterSet,
    build_counter_hierarchy,
)
from turnip_text.render.latex.renderer import LatexRenderer


def STD_LATEX_RENDER_PLUGINS(
    use_chapters: bool,
    indent_list_items: bool = True,
    requested_counter_links: Optional[Dict[Optional[str], str]] = None,
) -> List[RenderPlugin[LatexRenderer]]:
    ps = [
        StructureRenderPlugin(use_chapters),
        UncheckedBiblatexRenderPlugin(),
        FootnoteRenderPlugin(),
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

    def _register_node_handlers(
        self, handlers: RendererHandlers[LatexRenderer]
    ) -> None:
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
    def _register_node_handlers(
        self, handlers: RendererHandlers[LatexRenderer]
    ) -> None:
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
    def _register_node_handlers(
        self, handlers: RendererHandlers[LatexRenderer]
    ) -> None:
        handlers.register_block_or_inline(FootnoteRef, self._emit_footnote)

    def _requested_counters(self) -> Iterable[CounterLink]:
        return ((None, "footnote"),)

    def _emit_footnote(
        self,
        footnote: FootnoteRef,
        renderer: LatexRenderer,
        ctx: FormatContext,
    ) -> None:
        f = renderer.doc.lookup_float_from_backref(footnote.backref)
        if f is None:
            raise ValueError(f"Reference to nonexistant footnote {footnote.backref}")
        renderer.emit_macro("footnote")
        renderer.emit_braced(f)


class ListRenderPlugin(RenderPlugin[LatexRenderer]):
    indent_list_items: bool = True

    def __init__(self, indent_list_items: bool = True):
        self.indent_list_items = indent_list_items

    def _register_node_handlers(
        self, handlers: RendererHandlers[LatexRenderer]
    ) -> None:
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
    def _register_node_handlers(
        self, handlers: RendererHandlers[LatexRenderer]
    ) -> None:
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
    def _register_node_handlers(
        self, handlers: RendererHandlers[LatexRenderer]
    ) -> None:
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


# TODO could do something here with document.counted_anchor_kinds and comparing against supported anchors...
class AnchorCountingBackrefPlugin(RenderPlugin[LatexRenderer]):
    node_counters: Dict[
        int, CounterChainValue
    ]  # Mapping of <node id> -> <counter value for node>
    anchor_counters: Dict[
        Tuple[str, str], CounterChainValue
    ]  # Mapping of (kind, id) for <counter value>

    def __init__(
        self,
        counter_links: List[CounterLink],
        other_plugins: List[RenderPlugin[LatexRenderer]],
    ) -> None:
        super().__init__()

        for p in other_plugins:
            counter_links.extend(p._requested_counters())

        self.counters = CounterSet(build_counter_hierarchy(counter_links))
        self.node_counters = {}
        self.anchor_counters = {}

    # TODO add dependency on cleveref?
    def _register_node_handlers(
        self, handlers: RendererHandlers[LatexRenderer]
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
    def lookup_anchorable_name(self, node: Any) -> Inline:
        raise NotImplementedError()

    def _emit_backref(
        self,
        backref: Backref,
        renderer: LatexRenderer,
        fmt: FormatContext,
    ):
        # TODO branch based on anchor kind - e.g. backrefs directly to text should use \pageref{}
        # TODO if the backref has label_contents, respect that
        renderer.emit_macro("cref")
        renderer.emit_braced(renderer.doc.anchors.lookup_backref(backref).canonical())

    def _emit_anchor(
        self,
        anchor: Anchor,
        renderer: LatexRenderer,
        fmt: FormatContext,
    ):
        # TODO branch based on anchor kind
        # TODO is this right? we don't need to label anything if it doesn't have a referrable anchor
        if anchor.id:
            renderer.emit_macro("label")
            renderer.emit_braced(anchor.canonical())

    def get_anchor_counter(self, a: Anchor) -> Optional[CounterChainValue]:
        if a.id is None:
            return None
        return self.anchor_counters.get((a.kind, a.id), None)

    def get_node_counter(self, n: Any) -> Optional[CounterChainValue]:
        return self.node_counters.get(id(n), None)
