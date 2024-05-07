from typing import Dict, Iterator, List, Optional

from turnip_text import BlockScope, DocSegment, Raw
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
from turnip_text.env_plugins import FmtEnv
from turnip_text.render import RenderPlugin
from turnip_text.render.latex.backrefs import LatexBackrefMethod
from turnip_text.render.latex.renderer import LatexCounterStyle, LatexRenderer
from turnip_text.render.latex.setup import LatexCounterDecl, LatexSetup
from turnip_text.render.manual_numbering import SimpleCounterFormat

LatexPlugin = RenderPlugin[LatexSetup]


def STD_LATEX_ARTICLE_RENDER_PLUGINS(
    use_chapters: bool,
    indent_list_items: bool = True,
) -> List[LatexPlugin]:
    return [
        LatexStructurePlugin_Article(use_chapters),
        LatexBiblatexPlugin_Unchecked(),
        LatexFootnotePlugin(),
        LatexListPlugin(indent_list_items),
        LatexInlineFormatPlugin(),
        LatexUrlPlugin(),
        LatexSubfilePlugin(),
    ]


class LatexStructurePlugin_Article(LatexPlugin, StructureEnvPlugin):
    level_to_latex: List[Optional[str]]

    # TODO this might need to enable \part?

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

    def _register(self, setup: LatexSetup) -> None:
        setup.require_document_class("article")
        # TODO enable more backref methods
        backref_methods = (LatexBackrefMethod.Cleveref, LatexBackrefMethod.Hyperlink)
        # Declare the preexisting LaTeX counters
        counters = [
            (None, "part"),
            ("part", "chapter"),
            ("chapter", "section"),
            ("section", "subsection"),
            ("subsection", "subsubsection"),
        ]
        for parent, counter in counters:
            setup.declare_latex_counter(
                counter,
                LatexCounterDecl(
                    provided_by_docclass_or_package=True,
                    default_reset_latex_counter=parent,
                    fallback_fmt=SimpleCounterFormat(counter, LatexCounterStyle.Arabic),
                ),
                backref_methods,
            )
        # Map the turnip_text counters to the LaTeX counter
        for i in [1, 2, 3, 4]:
            tt_counter = f"h{i}"
            if i < len(self.level_to_latex):
                latex_counter = self.level_to_latex[i]
                if latex_counter is not None:
                    setup.declare_tt_counter(tt_counter, latex_counter)

        setup.emitter.register_header(StructureHeader, self._emit_structure)

    def _emit_structure(
        self,
        head: StructureHeader,
        contents: BlockScope,
        subsegments: Iterator[DocSegment],
        renderer: LatexRenderer,
        fmt: FmtEnv,
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
        renderer.emit_braced(head.title)  # i.e. r"\section*" + "{Section Name}"
        if head.anchor:
            renderer.emit(
                head.anchor
            )  # i.e. r"\section*{Section Name}\label{h1:Section_Name}"
        renderer.emit_break_paragraph()
        # Now emit the rest of the damn doc :)
        renderer.emit_blockscope(contents)
        for s in subsegments:
            renderer.emit_segment(s)


class LatexBiblatexPlugin_Unchecked(LatexPlugin, CitationEnvPlugin):
    def _register(self, setup: LatexSetup) -> None:
        setup.emitter.register_block_or_inline(Citation, self._emit_cite)
        setup.emitter.register_block_or_inline(CiteAuthor, self._emit_citeauthor)
        setup.emitter.register_block_or_inline(Bibliography, self._emit_bibliography)

    def _emit_cite(self, cite: Citation, renderer: LatexRenderer, fmt: FmtEnv) -> None:
        renderer.emit_macro("cite")
        if cite.citenote:
            renderer.emit_sqr_bracketed(cite.citenote)
        renderer.emit_braced(Raw(",".join(cite.citekeys)))

    def _emit_citeauthor(
        self,
        citeauthor: CiteAuthor,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        renderer.emit_raw(f"\\citeauthor{{{citeauthor.citekey}}}")

    def _emit_bibliography(
        self,
        bib: Bibliography,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        renderer.emit_raw("{")
        renderer.emit_break_sentence()
        with renderer.indent(4):
            renderer.emit_raw("\\raggedright")
            renderer.emit_break_sentence()
            renderer.emit_raw("\\printbibliography[heading=none]")
            renderer.emit_break_sentence()
        renderer.emit_raw("}")
        renderer.emit_break_paragraph()


class LatexFootnotePlugin(LatexPlugin, FootnoteEnvPlugin):
    def _register(self, setup: LatexSetup) -> None:
        setup.emitter.register_block_or_inline(FootnoteRef, self._emit_footnote)
        setup.emitter.register_block_or_inline(
            FootnoteContents, lambda _, __, ___: None
        )
        # This internally uses the footnote counter but it's a *magic* counter that doesn't correspond 1:1 to a turnip_text counter in value
        # For example the value is page dependent
        # => don't treat it as a normal counter
        setup.declare_magic_tt_and_latex_counter(
            tt_counter="footnote", latex_counter="footnote"
        )

    def _emit_footnote(
        self,
        footnote: FootnoteRef,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        footnote_backref = footnote.portal_to
        _, footnote_contents = renderer.anchors.lookup_backref_float(footnote_backref)
        if footnote_contents is None:
            raise ValueError(f"Reference to nonexistant footnote {footnote_backref}")
        assert isinstance(footnote_contents, FootnoteContents)
        renderer.emit_macro("footnote")
        renderer.emit_braced(footnote_contents.contents)


class LatexListPlugin(LatexPlugin, ListEnvPlugin):
    indent_list_items: bool = True

    def __init__(self, indent_list_items: bool = True):
        self.indent_list_items = indent_list_items

    def _register(self, setup: LatexSetup) -> None:
        setup.emitter.register_block_or_inline(DisplayList, self._emit_list)
        setup.emitter.register_block_or_inline(DisplayListItem, self._emit_list_item)

    def _emit_list(
        self,
        list: DisplayList,
        renderer: LatexRenderer,
        fmt: FmtEnv,
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
        fmt: FmtEnv,
    ) -> None:
        # Put {} after \item so square brackets at the start of render_block don't get swallowed as arguments
        renderer.emit_raw("\\item{} ")
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


class LatexInlineFormatPlugin(LatexPlugin, InlineFormatEnvPlugin):
    # TODO If we don't use squotes,dquotes manually it would make sense to use enquote from csquotes package
    def _register(self, setup: LatexSetup) -> None:
        setup.emitter.register_block_or_inline(InlineFormatted, self._emit_formatted)

    def _emit_formatted(
        self,
        f: InlineFormatted,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        if f.format_type == InlineFormattingType.SingleQuote:
            renderer.emit_raw("`")
            renderer.emit_inlinescope(f.contents)
            renderer.emit_raw("'")
        elif f.format_type == InlineFormattingType.DoubleQuote:
            renderer.emit_raw("``")
            renderer.emit_inlinescope(f.contents)
            renderer.emit_raw("''")
        else:
            # All other kinds are just the contents wrapped in a macro
            renderer.emit_macro(FORMAT_TYPE_TO_MACRO[f.format_type])
            renderer.emit_braced(f.contents)


class LatexUrlPlugin(LatexPlugin, UrlEnvPlugin):
    def _register(self, setup: LatexSetup) -> None:
        setup.request_latex_package("hyperref", "URL rendering")
        setup.emitter.register_block_or_inline(NamedUrl, self._emit_url)

    def _emit_url(
        self,
        url: NamedUrl,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        if "}" in url.url:
            raise RuntimeError(
                f"Can't handle url {url.url} with a }} in it. Please use proper percent-encoding to escape it."
            )

        # TODO this breaks if the hash is already escaped :|

        if url.name is None:
            renderer.emit_macro("url")
            renderer.emit_braced(Raw(url.url.replace("#", "\\#")))
        else:
            renderer.emit_macro("href")
            renderer.emit_braced(Raw(url.url.replace("#", "\\#")))
            renderer.emit_braced(*url.name)


class LatexSubfilePlugin(LatexPlugin, SubfileEnvPlugin):
    pass
