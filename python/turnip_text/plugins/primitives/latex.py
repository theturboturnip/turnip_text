from typing import Any

from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.helpers import (
    NullRawBuilder,
    PassthroughRawBuilder,
    UserRawScopeBuilder,
)
from turnip_text.plugins.primitives import PageBreak, PrimitivesPlugin
from turnip_text.render.latex.renderer import LatexRenderer
from turnip_text.render.latex.setup import LatexPlugin, LatexSetup


class LatexPrimitivesPlugin(LatexPlugin, PrimitivesPlugin):
    # TODO add a preamble=True flag to append code to the preamble?
    def raw(self, lang: str, **kwargs: Any) -> UserRawScopeBuilder:
        lang = lang.lower().strip()
        if lang in ["tex", "latex"]:
            return PassthroughRawBuilder()
        else:
            return NullRawBuilder()

    def _register(self, build_sys: BuildSystem, setup: LatexSetup) -> None:
        setup.emitter.register_block_or_inline(PageBreak, self._emit_page_break)

    def _emit_page_break(
        self, pb: PageBreak, renderer: LatexRenderer, fmt: FmtEnv
    ) -> None:
        if pb.double:
            renderer.emit_macro("cleardoublepage")
        else:
            renderer.emit_macro("clearpage")
