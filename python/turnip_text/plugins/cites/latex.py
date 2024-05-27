from typing import List, Optional, Tuple, Type, Union

from typing_extensions import override

from turnip_text import Inline, Raw
from turnip_text.build_system import BuildSystem, InputRelPath, OutputRelPath
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
        renderer.emit_raw("{\n")
        with renderer.indent(4):
            renderer.emit_raw("\\raggedright\n")
            renderer.emit_raw("\\printbibliography[heading=none]\n")
        renderer.emit_raw("}")
        renderer.emit_break_paragraph()


class LatexBiblatexCitationPlugin(LatexPlugin, CitationEnvPlugin):
    _biblatex_path: InputRelPath
    _db_type: Type[BibLatexCitationDB]
    _citation_db: BibLatexCitationDB
    _minimal_bib_name: Optional[OutputRelPath]

    def __init__(
        self,
        bibtex_path: Optional[InputRelPath] = None,
        output_bib_name: Optional[OutputRelPath] = None,
        # Parameterizable db_type if you want to override functions in citation DB
        db_type: Type[BibLatexCitationDB]=BibLatexCitationDB,
    ) -> None:
        if bibtex_path:
            self._biblatex_path = bibtex_path
        else:
            raise ValueError(f"Specify bibtex_path")

        self._minimal_bib_name = output_bib_name
        self._db_type = db_type

    def _register(self, build_sys: BuildSystem, setup: LatexSetup) -> None:
        self._citation_db = self._db_type(build_sys, [self._biblatex_path])

        if self._minimal_bib_name:
            # Write out the bibliography once we know the exact set of items we want in it
            build_sys.defer_supplementary_file(self._write_minimal_citation_db)

        setup.package_resolver.request_latex_package(
            "csquotes", reason="bibliography (for babel and biblatex)"
        )
        # FUTURE multilingual support shouldn't just pass british in here
        setup.package_resolver.request_latex_package(
            "babel", reason="bibliography", options=["british"]
        )
        setup.package_resolver.request_latex_package(
            "biblatex",
            reason="bibliography",
            options=[
                ("backend", "biber"),
                ("dateabbrev", "false"),
                ("style", "numeric"),
            ],
        )

        setup.emitter.register_block_or_inline(Citation, self._emit_citation)
        setup.emitter.register_block_or_inline(CiteAuthor, self._emit_citeauthor)
        setup.emitter.register_block_or_inline(Bibliography, self._emit_bibliography)

        setup.add_preamble_section(self._emit_preamble)

    def _write_minimal_citation_db(self, build_sys: BuildSystem) -> None:
        assert (
            self._minimal_bib_name
        ), "Shouldn't be calling _write_minimal_citation_db if there isn't a path for the minimal citation db"
        with build_sys.resolve_output_file(
            self._minimal_bib_name
        ).open_write_text() as f:
            self._citation_db.write_minimal_db(f)

    def _make_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]] | None:
        def visit_cite_or_citeauthor(c: Union[Citation, CiteAuthor]) -> None:
            if isinstance(c, CiteAuthor):
                self._citation_db.register_entry_used(c.citekey)
            else:
                for k in c.citekeys:
                    self._citation_db.register_entry_used(k)

        return [((Citation, CiteAuthor), visit_cite_or_citeauthor)]

    def _emit_preamble(self, renderer: LatexRenderer) -> None:
        if self._minimal_bib_name:
            renderer.emit_comment_headline(
                f"Setup bibliography for {self.__class__.__name__}"
            )
            renderer.emit_macro("addbibresource")
            renderer.emit_braced(Raw(str(self._minimal_bib_name)))
        else:
            renderer.emit_comment_headline(
                "Not including bibfile for {self.__class__.__name__}, because none was generated."
            )
            renderer.emit_comment_line(
                f"It is your responsibility to copy '{self._biblatex_path}' somewhere into the output directory,"
            )
            renderer.emit_comment_line("and change this LaTeX to include it manually.")

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
        renderer.emit_raw("{")
        with renderer.indent(4):
            renderer.emit_newline()
            renderer.emit_macro("raggedright")
            renderer.emit_newline()
            renderer.emit_raw("\\printbibliography[heading=none]")
            renderer.emit_newline()
        renderer.emit_raw("}")
        renderer.emit_newline()

    @override
    def register_raw_cite(self, *citekeys: str) -> None:
        for k in citekeys:
            self._citation_db.register_entry_used(k)
