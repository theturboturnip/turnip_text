from typing import Iterator, List, Literal, Optional, Tuple, cast

from turnip_text import BlockScope, DocSegment, Raw, Text, join_inlines
from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.doc_structure import (
    AppendixHeader,
    BasicHeader,
    StructureEnvPlugin,
    TableOfContents,
    TitleBlock,
)
from turnip_text.render.latex.backrefs import LatexBackrefMethod
from turnip_text.render.latex.counter_resolver import LatexCounterDecl
from turnip_text.render.latex.renderer import LatexCounterStyle, LatexRenderer
from turnip_text.render.latex.setup import LatexPlugin, LatexSetup
from turnip_text.render.manual_numbering import SimpleCounterFormat

StartLatexHeader = (
    Literal["chapter"]
    | Literal["section"]
    | Literal["subsection"]
    | Literal["subsubsection"]
    | Literal["paragraph"]
    | Literal["subparagraph"]
)

BasicLatexDocClass = Literal["article"] | Literal["report"] | Literal["book"]


class LatexDocumentClassPlugin_Basic(LatexPlugin, StructureEnvPlugin):
    """Defines rendering for structure headers in the three basic document classes: 'article', 'report', and 'book'.

    - Headers with weight=0 are always \part. TODO make these available
    - Headers with weight=1 are the macro specified by the h1 argument,
      and headers with greater weights follow in this order:

      - chapter (not available in 'article' document class)
      - section
      - subsection
      - subsubsection
      - paragraph
      - subparagraph

    See https://anorien.csc.warwick.ac.uk/mirrors/CTAN/macros/latex/base/classes.pdf"""

    # TODO make "part" consistent with markdown - how on earth is "part" supposed to work

    level_to_latex: List[str] = [
        "part",
        "chapter",
        "section",
        "subsection",
        "subsubsection",
        "paragraph",
        "subparagraph",
    ]
    doc_class: BasicLatexDocClass

    def __init__(
        self,
        h1: StartLatexHeader,
        doc_class: BasicLatexDocClass = "article",
    ) -> None:
        super().__init__()
        if doc_class == "article" and h1 == "chapter":
            raise ValueError("'chapter's are not available in document class 'article'")
        # Generate a list like level_to_latex but where index [1] is the value specified in h1.
        self.level_to_latex = LatexDocumentClassPlugin_Basic.level_to_latex[
            LatexDocumentClassPlugin_Basic.level_to_latex.index(h1) - 1 :
        ]
        self.level_to_latex[0] = "part"
        if doc_class == "article" and "chapter" in self.level_to_latex:
            self.level_to_latex.remove("chapter")

        self.doc_class = doc_class

    def _register(self, build_sys: BuildSystem, setup: LatexSetup) -> None:
        setup.require_document_class(self.doc_class)

        setup.add_preamble_section(self._emit_metadata_preamble)

        # TODO enable more backref methods
        backref_methods = (
            LatexBackrefMethod.Cleveref,
            LatexBackrefMethod.Hyperlink,
            LatexBackrefMethod.ManualRef,
        )
        # Declare the preexisting LaTeX counters
        counters = [
            (None, "part"),
            ("section", "subsection"),
            ("subsection", "subsubsection"),
            ("subsubsection", "paragraph"),
            ("paragraph", "subparagraph"),
        ]
        if self.doc_class == "article":
            counters.append((None, "section"))
        else:
            counters.extend([(None, "chapter"), ("chapter", "section")])
        for parent, counter in counters:
            setup.counter_resolver.declare_latex_counter(
                counter,
                LatexCounterDecl(
                    provided_by_docclass_or_package=True,
                    default_reset_latex_counter=parent,
                    default_fmt=SimpleCounterFormat(
                        counter,
                        (
                            LatexCounterStyle.RomanUpper
                            if counter == "part"
                            else LatexCounterStyle.Arabic
                        ),
                    ),
                ),
                backref_methods,
            )
        # Map the turnip_text counters from h0 to hN to the LaTeX counter
        for i in range(len(self.level_to_latex)):
            tt_counter = f"h{i}"
            latex_counter = self.level_to_latex[i]
            if latex_counter is not None:
                setup.counter_resolver.declare_tt_counter(tt_counter, latex_counter)

        # appendix counter
        setup.counter_resolver.declare_latex_counter(
            "appendix",
            LatexCounterDecl(
                provided_by_docclass_or_package=False,
                default_reset_latex_counter=None,
                default_fmt=SimpleCounterFormat(
                    "Appendix", LatexCounterStyle.AlphUpper
                ),
            ),
            backref_methods,
        )
        setup.counter_resolver.declare_tt_counter("appendix", "appendix")

        setup.emitter.register_header(BasicHeader, self._emit_structure_header)
        setup.emitter.register_header(AppendixHeader, self._emit_appendix_header)
        setup.emitter.register_block_or_inline(TableOfContents, self._emit_toc)
        setup.emitter.register_block_or_inline(TitleBlock, self._emit_title)

        # TODO make this only supply an option to hyperref, not implicitly include it
        setup.package_resolver.request_latex_package(
            "hyperref", "PDF metadata", ["pdfusetitle", "unicode"]
        )

    def _emit_metadata_preamble(self, renderer: LatexRenderer) -> None:
        renderer.emit_comment_headline(
            "Setting up metadata from " + self.__class__.__name__
        )
        if self._metadata:
            if self._metadata.title or self._metadata.subtitle:
                renderer.emit_comment_line("Combined title and subtitle")
                renderer.emit_raw("\\title{")
                if self._metadata.title:
                    renderer.emit(self._metadata.title)
                    if self._metadata.subtitle:
                        # "\\" to put the subtitle on a separate line
                        renderer.emit_raw("\\\\")
                if self._metadata.subtitle:
                    renderer.emit_braced(Raw("\large "), self._metadata.subtitle)
                renderer.emit_raw("}\n")
            else:
                renderer.emit_comment_line("No title or subtitle supplied")

            if self._metadata.authors:
                renderer.emit_macro("author")
                renderer.emit_braced(join_inlines(self._metadata.authors, Text(", ")))
                renderer.emit_break_sentence()
            else:
                renderer.emit_comment_line("No authors supplied")
        else:
            renderer.emit_comment_line("No metadata supplied")

    def _emit_toc(
        self,
        toc: TableOfContents,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        if toc.depth < 0:
            return

        # Figure out the LaTeX depth to set \tocdepth to
        # Take our turnip_text depth, convert it to a LaTeX macro, and map that macro to LaTeX depth.
        LATEX_TO_DEPTH = {
            "part": -1,
            "chapter": 0,
            "section": 1,
            "subsection": 2,
            "subsubsection": 3,
            "paragraph": 4,
            "subparagraph": 5,
        }
        requested_latex_depth = LATEX_TO_DEPTH[self.level_to_latex[toc.depth]]
        renderer.emit_macro("setcounter")
        renderer.emit_braced(Raw("tocdepth"))
        renderer.emit_braced(Raw(str(requested_latex_depth)))
        renderer.emit_break_sentence()
        # Actually get the tableofcontents
        renderer.emit_macro("tableofcontents")
        renderer.emit_break_paragraph()

    def _emit_title(
        self,
        title_block: TitleBlock,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        renderer.emit_macro("maketitle")
        renderer.emit_break_sentence()

    def _emit_structure_header(
        self,
        head: BasicHeader,
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

    def _emit_appendix_header(
        self,
        head: AppendixHeader,
        contents: BlockScope,
        subsegments: Iterator[DocSegment],
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        # Step the LaTeX counter - do this before the section header so that when you click it zips to the start of the header.
        renderer.emit_comment_headline(
            "New appendix - refstep the counter here so the label attaches to it"
        )
        # This is imperfect - FUTURE better appendix counter
        renderer.emit_raw("\\refstepcounter{appendix}")
        renderer.emit(
            head.anchor
        )  # i.e. r"\refstepcounter{appendix}\label{appendix:1}"
        # TODO This doesn't appear in the ToC at allllllllllll
        # Emit \chapter* or \section* with the counter hardcoded
        latex_name = "section" if self.doc_class == "article" else "chapter"
        renderer.emit_macro(latex_name + "*")
        renderer.emit_braced(
            renderer.get_resolved_anchor_text(head.anchor),
            Text(" "),
            fmt.emdash,
            Text(" "),
            head.title,
        )  # i.e. r"\section*" + "{Appendix A --- Section Name}"
        renderer.emit_break_paragraph()
        # Now emit the rest of the damn doc :)
        renderer.emit_blockscope(contents)
        for s in subsegments:
            renderer.emit_segment(s)
