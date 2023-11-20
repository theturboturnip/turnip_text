from typing import Iterable, Iterator, List, Optional

from turnip_text import Block, BlockScope, DocSegment
from turnip_text.doc import FormatContext
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
from turnip_text.render import RendererHandlers, RenderPlugin
from turnip_text.render.latex.renderer import LatexRenderer


def STD_LATEX_RENDER_PLUGINS(use_chapters: bool, indent_list_items: bool=True) -> List[RenderPlugin[LatexRenderer]]:
    return [
        StructureRenderPlugin(use_chapters),
        UncheckedBiblatexRenderPlugin(),
        FootnoteRenderPlugin(),
        ListRenderPlugin(indent_list_items),
        InlineFormatRenderPlugin(),
        UrlRenderPlugin(),        
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
            self.level_to_latex = [
                None,
                "section",
                "subsection",
                "subsubsection"
            ]

    def _register_node_handlers(self, handlers: RendererHandlers[LatexRenderer]) -> None:
        handlers.register_header_renderer(StructureBlockHeader, self._emit_structure)

    def _emit_structure(self, head: StructureBlockHeader, contents: BlockScope, subsegments: Iterator[DocSegment], vitis: None, renderer: LatexRenderer, ctx: FormatContext):
        latex_name = self.level_to_latex[head.weight]
        if latex_name is None:
            raise ValueError(f"Can't emit {head} because it uses an unusable weight: {head.weight}")
        if head.anchor:
            # This is a numbered entry with a label
            renderer.emit_macro(latex_name) # i.e. r"\section"
        else:
            renderer.emit_macro(latex_name + "*")
        renderer.emit_braced(head.contents) # i.e. r"\section*" + "{Section Name}"
        if head.anchor:
            renderer.emit_macro("label")
            renderer.emit_braced(head.anchor.canonical()) # i.e. r"\section*{Section Name}\label{h1:Section_Name}"
        renderer.emit_break_paragraph()
        # Now emit the rest of the damn doc :)
        renderer.emit_blockscope(contents)
        for s in subsegments:
            renderer.emit_segment(s)

class UncheckedBiblatexRenderPlugin(RenderPlugin[LatexRenderer]):
    def _register_node_handlers(self, handlers: RendererHandlers[LatexRenderer]) -> None:
        handlers.register_block_or_inline_renderer(Citation, self._emit_cite)
        handlers.register_block_or_inline_renderer(CiteAuthor, self._emit_citeauthor)
        handlers.register_header_renderer(Bibliography, self._emit_bibliography)

    def _emit_cite(self, cite: Citation, visit: None, renderer: LatexRenderer, ctx: FormatContext):
        renderer.emit_macro("cite")
        if cite.contents:
            renderer.emit_sqr_bracketed(cite.contents)
        renderer.emit_braced(",".join(cite.citekeys))

    def _emit_citeauthor(self, citeauthor: CiteAuthor, visit: None, renderer: LatexRenderer, ctx: FormatContext):
        renderer.emit_macro("citeauthor")
        renderer.emit_braced(citeauthor.citekey)

    def _emit_bibliography(self, bib: Bibliography, block_contents: BlockScope, segment_contents: Iterator[DocSegment], visit: None, renderer: LatexRenderer, ctx: FormatContext):
        renderer.emit("{")
        with renderer.indent(4):
            renderer.emit("\\raggedright")
            renderer.emit_break_sentence()
            renderer.emit("\\printbibliography")
            renderer.emit_break_sentence()
        renderer.emit("}")
        renderer.emit_break_paragraph()
        renderer.emit(block_contents, *segment_contents)

class FootnoteRenderPlugin(RenderPlugin[LatexRenderer]):
    def _register_node_handlers(self, handlers: RendererHandlers[LatexRenderer]) -> None:
        # TODO use visitor pattern for this? Right now the visitor doesn't get any other arguments
        # handlers.register_block_or_inline(FootnoteRef, self._visit_footnote, self._emit_footnote)
        handlers.register_block_or_inline_renderer(FootnoteRef, self._emit_footnote)

    # def _visit_footnote(self, footnote: FootnoteRef) -> Block:
    #     return None

    def _emit_footnote(self, footnote: FootnoteRef, visit: None, renderer: LatexRenderer, ctx: FormatContext) -> None:
        f = renderer.doc.lookup_float_from_backref(footnote.ref)
        if f is None:
            raise ValueError(f"Reference to nonexistant footnote {footnote.ref}")
        renderer.emit_macro("footnote")
        renderer.emit_braced(f)

class ListRenderPlugin(RenderPlugin[LatexRenderer]):
    indent_list_items: bool = True

    def __init__(self, indent_list_items: bool = True):
        self.indent_list_items = indent_list_items

    def _register_node_handlers(self, handlers: RendererHandlers[LatexRenderer]) -> None:
        handlers.register_block_or_inline_renderer(DisplayList, self._emit_list)
        handlers.register_block_or_inline_renderer(DisplayListItem, self._emit_list_item)

    def _emit_list(
        self,
        list: DisplayList,
        visit: None,
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
        visit: None,
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
    InlineFormattingType.Strong: "strong"
}

class InlineFormatRenderPlugin(RenderPlugin[LatexRenderer]):
    # TODO enquote/csquotes package?
    def _register_node_handlers(self, handlers: RendererHandlers[LatexRenderer]) -> None:
        handlers.register_block_or_inline_renderer(InlineFormatted, self._emit_formatted)

    def _emit_formatted(self, f: InlineFormatted, visit: None, renderer: LatexRenderer, fmt: FormatContext) -> None:
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
    def _register_node_handlers(self, handlers: RendererHandlers[LatexRenderer]) -> None:
        handlers.register_block_or_inline_renderer(NamedUrl, self._emit_url)

    def _emit_url(
        self, url: NamedUrl, visit: None, renderer: LatexRenderer, fmt: FormatContext, 
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