from dataclasses import dataclass
from typing import Any, Sequence

from turnip_text import Block, Header, Inline, Raw, RawScopeBuilder
from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.helpers import (
    NullRawBuilder,
    PassthroughRawBuilder,
    UserRawScopeBuilder,
)
from turnip_text.plugins.primitives import PageBreak, PrimitivesPlugin
from turnip_text.render.markdown.renderer import (
    MarkdownPlugin,
    MarkdownRenderer,
    MarkdownSetup,
)


@dataclass(frozen=True)
class MarkdownOnlyRaw(Inline):
    """A version of Raw that throws an error if emitted in HTML-mode"""

    data: str


class MarkdownOnlyRawWrapper(UserRawScopeBuilder):
    """Given a Raw, take the data and wrap it in MarkdownOnlyRaw"""

    def build_from_raw(self, r: Raw) -> Header | Block | Inline | None:
        return MarkdownOnlyRaw(r.data)


class MarkdownPrimitivesPlugin(MarkdownPlugin, PrimitivesPlugin):
    def raw(self, lang: str, **kwargs: Any) -> UserRawScopeBuilder:
        lang = lang.lower().strip()
        if lang in ["md", "markdown"]:
            # Markdown is only allowed in contexts where we aren't doing HTML.
            return MarkdownOnlyRawWrapper()
        elif lang == "html":
            # HTML is always allowed inside markdown
            return PassthroughRawBuilder()
        else:
            return NullRawBuilder()

    def _doc_nodes(self) -> Sequence[type[Block] | type[Inline] | type[Header]]:
        return list(super()._doc_nodes()) + [MarkdownOnlyRaw]

    def _register(self, build_sys: BuildSystem, setup: MarkdownSetup) -> None:
        setup.emitter.register_block_or_inline(MarkdownOnlyRaw, self._emit_markdown_raw)
        setup.emitter.register_block_or_inline(PageBreak, self._emit_page_break)

    def _emit_markdown_raw(
        self, raw: MarkdownOnlyRaw, renderer: MarkdownRenderer, fmt: FmtEnv
    ) -> None:
        if renderer.in_html_mode:
            raise RuntimeError(
                f"Tried to emit a Markdown-only Raw object when in HTML mode - this will result in invalid syntax.\n{raw}"
            )
        renderer.emit_raw(raw.data)

    def _emit_page_break(
        self, pb: PageBreak, renderer: MarkdownRenderer, fmt: FmtEnv
    ) -> None:
        # https://developer.mozilla.org/en-US/docs/Web/CSS/break-after#syntax
        # FUTURE could be better to put this in a `@media print` or something
        if pb.double:
            renderer.emit_empty_tag("div", "style='break-after: recto;'")
        else:
            renderer.emit_empty_tag("div", "style='break-after: page;'")
