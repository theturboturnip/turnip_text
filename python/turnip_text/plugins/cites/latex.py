from typing import List, Optional, Tuple, Union

from typing_extensions import override

from turnip_text import Inline, Raw
from turnip_text.build_system import (
    BuildSystem,
    OutputRelativePath,
    ProjectRelativePath,
)
from turnip_text.doc.dfs import VisitorFilter, VisitorFunc
from turnip_text.env_plugins import DocEnv, FmtEnv, in_doc
from turnip_text.plugins.cites import (
    Bibliography,
    Citation,
    CitationEnvPlugin,
    CiteAuthor,
)
from turnip_text.plugins.cites.bib_database.bibtex import BibLatexCitationDB
from turnip_text.render.latex.renderer import LatexRenderer
from turnip_text.render.latex.setup import LatexPlugin, LatexSetup


class LatexBiblatexPlugin_Unchecked(LatexPlugin, CitationEnvPlugin):
    def _register(self, build_sys: BuildSystem, setup: LatexSetup) -> None:
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


class LatexBiblatexCitationPlugin(LatexPlugin, CitationEnvPlugin):
    _bibtex_path: ProjectRelativePath
    _citation_db: BibLatexCitationDB
    _minimal_bib_name: Optional[OutputRelativePath]

    def __init__(
        self,
        bibtex_path: Optional[ProjectRelativePath] = None,
        output_bib_name: Optional[OutputRelativePath] = None,
    ) -> None:
        if bibtex_path:
            self._bibtex_path = bibtex_path
        else:
            raise ValueError(f"Specify bibtex_path")

        self._minimal_bib_name = output_bib_name

        # TODO correct preamble

    def _register(self, build_sys: BuildSystem, setup: LatexSetup) -> None:
        self._citation_db = BibLatexCitationDB(build_sys, [self._bibtex_path])

        if self._minimal_bib_name:
            build_sys.register_file_generator(
                self._citation_db.write_minimal_db_job(),
                inputs={},
                output_relative_path=self._minimal_bib_name,
            )

        setup.request_latex_package(
            "csquotes", reason="bibliography (for babel and biblatex)"
        )
        setup.request_latex_package("babel", reason="bibliography")
        setup.request_latex_package("biblatex", reason="bibliography")

        setup.emitter.register_block_or_inline(Citation, self._emit_citation)
        setup.emitter.register_block_or_inline(CiteAuthor, self._emit_citeauthor)
        setup.emitter.register_block_or_inline(Bibliography, self._emit_bibliography)

    def _make_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]] | None:
        def visit_cite_or_citeauthor(c: Union[Citation, CiteAuthor]) -> None:
            if isinstance(c, CiteAuthor):
                self._citation_db.register_entry_used(c.citekey)
            else:
                for k in c.citekeys:
                    self._citation_db.register_entry_used(k)

        return [((Citation, CiteAuthor), visit_cite_or_citeauthor)]

    def _emit_citation(
        self,
        citation: Citation,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        renderer.emit_macro("cite")
        if citation.citenote is not None:
            renderer.emit_sqr_bracketed(citation.citenote)
        renderer.emit_braced(Raw(",".join(citation.citekeys)))

    def _emit_citeauthor(
        self,
        citation: CiteAuthor,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        renderer.emit_macro("citeauthor")
        renderer.emit_braced(Raw(citation.citekey))

    def _emit_bibliography(
        self,
        bibliography: Bibliography,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        renderer.emit_newline()
        renderer.emit_macro("pagebreak")
        renderer.emit_newline()
        renderer.emit_raw("{")
        with renderer.indent(4):
            renderer.emit_newline()
            renderer.emit_macro("raggedright")
            renderer.emit_newline()
            renderer.emit_macro("printbibliography")
            renderer.emit_sqr_bracketed(Raw("heading=bibintoc"))
            renderer.emit_newline()
        renderer.emit_raw("}")
        renderer.emit_newline()

    @override
    def register_raw_cite(self, *citekeys: str) -> None:
        for k in citekeys:
            self._citation_db.register_entry_used(k)
