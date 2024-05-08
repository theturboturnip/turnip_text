from turnip_text import InlineScope
from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.url import NamedUrl, UrlEnvPlugin
from turnip_text.render.markdown.renderer import (
    MarkdownPlugin,
    MarkdownRenderer,
    MarkdownSetup,
)


class MarkdownUrlPlugin(MarkdownPlugin, UrlEnvPlugin):
    def _register(self, build_sys: BuildSystem, setup: MarkdownSetup) -> None:
        setup.emitter.register_block_or_inline(NamedUrl, self._emit_url)

    def _emit_url(
        self,
        url: NamedUrl,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        # FUTURE could use reference-style links?
        renderer.emit_url(url.url, InlineScope(list(url.name)) if url.name else None)
