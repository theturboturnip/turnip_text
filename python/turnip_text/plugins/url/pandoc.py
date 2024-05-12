import turnip_text.render.pandoc.pandoc_types as pan
from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.url import NamedUrl, UrlEnvPlugin
from turnip_text.render.pandoc import (
    PandocPlugin,
    PandocRenderer,
    PandocSetup,
    null_attr,
)


class PandocUrlPlugin(PandocPlugin, UrlEnvPlugin):
    def _register(self, build_sys: BuildSystem, setup: PandocSetup) -> None:
        super()._register(build_sys, setup)
        setup.makers.register_inline(NamedUrl, self._make_url)

    def _make_url(
        self, url: NamedUrl, renderer: PandocRenderer, fmt: FmtEnv
    ) -> pan.Link:
        if url.name:
            name = renderer.make_inline_scope_list(url.name)
        else:
            name = [pan.Str(url.url)]
        return pan.Link(null_attr(), name, (url.url, url.url))
