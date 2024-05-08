from turnip_text import Raw
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.cites import (
    Bibliography,
    Citation,
    CitationEnvPlugin,
    CiteAuthor,
)
from turnip_text.render.latex.renderer import LatexRenderer
from turnip_text.render.latex.setup import LatexPlugin, LatexSetup


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
