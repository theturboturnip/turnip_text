from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.footnote import (
    FootnoteContents,
    FootnoteEnvPlugin,
    FootnoteRef,
)
from turnip_text.render.latex.renderer import LatexRenderer
from turnip_text.render.latex.setup import LatexPlugin, LatexSetup


class LatexFootnotePlugin(LatexPlugin, FootnoteEnvPlugin):
    def _register(self, build_sys: BuildSystem, setup: LatexSetup) -> None:
        setup.emitter.register_block_or_inline(FootnoteRef, self._emit_footnote)
        setup.emitter.register_block_or_inline(
            FootnoteContents, lambda _, __, ___: None
        )
        # This internally uses the footnote counter but it's a *magic* counter that doesn't correspond 1:1 to a turnip_text counter in value
        # For example the value is page dependent
        # => don't treat it as a normal counter
        setup.counter_resolver.declare_magic_tt_and_latex_counter(
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
