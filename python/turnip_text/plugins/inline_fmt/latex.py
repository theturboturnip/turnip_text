from matplotlib import use

from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.inline_fmt import (
    InlineFormatEnvPlugin,
    InlineFormatted,
    InlineFormattingType,
)
from turnip_text.render.latex.renderer import LatexRenderer
from turnip_text.render.latex.setup import LatexPlugin, LatexSetup

FORMAT_TYPE_TO_MACRO = {
    InlineFormattingType.Bold: "textbf",
    InlineFormattingType.Italic: "textit",
    InlineFormattingType.Underline: "underline",
    InlineFormattingType.Emph: "emph",
    InlineFormattingType.Strong: "strong",
    InlineFormattingType.Mono: "texttt",
}

# TODO if not using csquotes the PDF bookmarks look weird


class LatexInlineFormatPlugin(LatexPlugin, InlineFormatEnvPlugin):
    _use_csquotes: bool = True

    def __init__(self, use_csquotes: bool = True) -> None:
        super().__init__()
        self._use_csquotes = use_csquotes

    # TODO If we don't use squotes,dquotes manually it would make sense to use enquote from csquotes package
    def _register(self, build_sys: BuildSystem, setup: LatexSetup) -> None:
        setup.emitter.register_block_or_inline(InlineFormatted, self._emit_formatted)
        if self._use_csquotes:
            setup.package_resolver.request_latex_package("csquotes", reason="Quotes")

            CSQUOTES_BOOKMARK_FIX = r"""
%% Fix PDF bookmarks to match csquotes https://tex.stackexchange.com/a/592412
\makeatletter
\DeclarePlainStyle{\csq@thequote@oopen}{\csq@thequote@oclose}{\csq@thequote@iopen}{\csq@thequote@iclose}
\pdfstringdefDisableCommands{\csq@resetstyle}
\makeatletter
"""
            setup.add_preamble_section(lambda r: r.emit_raw(CSQUOTES_BOOKMARK_FIX))

    def _emit_formatted(
        self,
        f: InlineFormatted,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        if f.format_type == InlineFormattingType.SingleQuote:
            if self._use_csquotes:
                renderer.emit_macro("enquote")
                renderer.emit_braced(f.contents)
            else:
                renderer.emit_raw("`")
                renderer.emit_inlinescope(f.contents)
                renderer.emit_raw("'")
        elif f.format_type == InlineFormattingType.DoubleQuote:
            if self._use_csquotes:
                renderer.emit_macro("enquote*")
                renderer.emit_braced(f.contents)
            else:
                renderer.emit_raw("``")
                renderer.emit_inlinescope(f.contents)
                renderer.emit_raw("''")
        else:
            # All other kinds are just the contents wrapped in a macro
            renderer.emit_macro(FORMAT_TYPE_TO_MACRO[f.format_type])
            renderer.emit_braced(f.contents)
