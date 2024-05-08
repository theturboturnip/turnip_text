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
}


class LatexInlineFormatPlugin(LatexPlugin, InlineFormatEnvPlugin):
    # TODO If we don't use squotes,dquotes manually it would make sense to use enquote from csquotes package
    def _register(self, build_sys: BuildSystem, setup: LatexSetup) -> None:
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
