import turnip_text.render.pandoc.pandoc_types as pan
from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.footnote import (
    FootnoteContents,
    FootnoteEnvPlugin,
    FootnoteRef,
)
from turnip_text.render.pandoc import PandocPlugin, PandocRenderer, PandocSetup


class PandocFootnotePlugin(PandocPlugin, FootnoteEnvPlugin):
    def _register(self, build_sys: BuildSystem, setup: PandocSetup) -> None:
        super()._register(build_sys, setup)
        setup.makers.register_inline(FootnoteRef, self._make_footnote)
        # FootnoteContents = a paragraph of the contents
        setup.makers.register_block(
            FootnoteContents,
            lambda fc, renderer, fmt: pan.Para([renderer.make_inline(fc.contents)]),
        )
        setup.define_unrenderable_counter("footnote")

    def _make_footnote(
        self, ref: FootnoteRef, renderer: PandocRenderer, fmt: FmtEnv
    ) -> pan.Note:
        _anchor, contents = renderer.anchors.lookup_backref_float(ref.portal_to)
        assert isinstance(contents, FootnoteContents)
        return pan.Note([renderer.make_block(contents)])
