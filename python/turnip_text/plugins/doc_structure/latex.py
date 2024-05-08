from typing import Iterator, List, Optional

from turnip_text import BlockScope, DocSegment
from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.doc_structure import StructureEnvPlugin, StructureHeader
from turnip_text.render.latex.backrefs import LatexBackrefMethod
from turnip_text.render.latex.renderer import LatexCounterStyle, LatexRenderer
from turnip_text.render.latex.setup import LatexCounterDecl, LatexPlugin, LatexSetup
from turnip_text.render.manual_numbering import SimpleCounterFormat


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

    def _register(self, build_sys: BuildSystem, setup: LatexSetup) -> None:
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
