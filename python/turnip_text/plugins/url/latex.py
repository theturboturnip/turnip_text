from turnip_text import Raw
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.url import NamedUrl, UrlEnvPlugin
from turnip_text.render.latex.renderer import LatexRenderer
from turnip_text.render.latex.setup import LatexPlugin, LatexSetup


class LatexUrlPlugin(LatexPlugin, UrlEnvPlugin):
    def _register(self, setup: LatexSetup) -> None:
        setup.request_latex_package("hyperref", "URL rendering")
        setup.emitter.register_block_or_inline(NamedUrl, self._emit_url)

    def _emit_url(
        self,
        url: NamedUrl,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        if "}" in url.url:
            raise RuntimeError(
                f"Can't handle url {url.url} with a }} in it. Please use proper percent-encoding to escape it."
            )

        # TODO this breaks if the hash is already escaped :|

        if url.name is None:
            renderer.emit_macro("url")
            renderer.emit_braced(Raw(url.url.replace("#", "\\#")))
        else:
            renderer.emit_macro("href")
            renderer.emit_braced(Raw(url.url.replace("#", "\\#")))
            renderer.emit_braced(*url.name)
