from typing import Iterator, List, Literal, Optional, Tuple

from turnip_text import BlockScope, DocSegment
from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.doc_structure import StructureEnvPlugin, StructureHeader
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

    level_to_latex: List[Optional[str]] = [
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
        base_weight: int = 1,
        doc_class: BasicLatexDocClass = "article",
    ) -> None:
        super().__init__()
        if doc_class == "article" and h1 == "chapter":
            raise ValueError(
                "'chapter's  are not available in document class 'article'"
            )
        # Generate a list like level_to_latex but where index [1] is the value specified in h1.
        # TODO handle the case where the base_weight is super high, e.g. if the base_weight = 5 and h1 = section
        # then it should be [None] * 4 + ["part"] + ["section"...]
        self.level_to_latex = LatexDocumentClassPlugin_Basic.level_to_latex[
            LatexDocumentClassPlugin_Basic.level_to_latex.index(h1) - base_weight :
        ]
        self.level_to_latex[0] = "part"
        if doc_class == "article" and "chapter" in self.level_to_latex:
            self.level_to_latex.remove("chapter")

        self.doc_class = doc_class

    def _register(self, build_sys: BuildSystem, setup: LatexSetup) -> None:
        setup.require_document_class(self.doc_class)
        # TODO enable more backref methods
        backref_methods = (LatexBackrefMethod.Cleveref, LatexBackrefMethod.Hyperlink)
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
        # Map the turnip_text counters from h1 to hN to the LaTeX counter
        # TODO make h() with arbitrary depth instead of h1..N
        for i in range(1, len(self.level_to_latex)):
            tt_counter = f"h{i}"
            latex_counter = self.level_to_latex[i]
            if latex_counter is not None:
                setup.counter_resolver.declare_tt_counter(tt_counter, latex_counter)

        setup.emitter.register_header(StructureHeader, self._emit_structure_header)

    def _emit_structure_header(
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
