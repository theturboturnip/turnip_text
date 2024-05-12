from typing import cast

from turnip_text import Text
from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.inline_fmt import (
    InlineFormatEnvPlugin,
    InlineFormatted,
    InlineFormattingType,
)
from turnip_text.render.markdown.renderer import (
    MarkdownPlugin,
    MarkdownRenderer,
    MarkdownSetup,
)

FORMAT_TYPE_TO_MARKDOWN = {
    InlineFormattingType.Bold: "**",
    InlineFormattingType.Italic: "*",
    InlineFormattingType.Emph: "*",  # = italic
    InlineFormattingType.Strong: "**",
}

FORMAT_TYPE_TO_HTML = {
    InlineFormattingType.Bold: "b",
    InlineFormattingType.Italic: "i",
    InlineFormattingType.Underline: "u",
    InlineFormattingType.Emph: "em",
    InlineFormattingType.Strong: "strong",
    InlineFormattingType.Mono: "code",
}


class MarkdownInlineFormatPlugin(MarkdownPlugin, InlineFormatEnvPlugin):
    def _register(self, build_sys: BuildSystem, setup: MarkdownSetup) -> None:
        setup.emitter.register_block_or_inline(InlineFormatted, self._emit_formatted)

    def _emit_formatted(
        self,
        f: InlineFormatted,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        if f.format_type == InlineFormattingType.SingleQuote:
            renderer.emit_raw("'")
            renderer.emit(f.contents)
            renderer.emit_raw("'")
        elif f.format_type == InlineFormattingType.DoubleQuote:
            renderer.emit_raw('"')
            renderer.emit(f.contents)
            renderer.emit_raw('"')
        elif renderer.in_html_mode:
            with renderer.emit_tag(FORMAT_TYPE_TO_HTML[f.format_type]):
                renderer.emit(f.contents)
        elif f.format_type == InlineFormattingType.Underline:
            # Have to go into html mode for this
            with renderer.emit_tag("u"):
                renderer.emit(f.contents)
        elif f.format_type == InlineFormattingType.Mono:
            if all(isinstance(i, Text) for i in f.contents):
                # Markdown ` doesn't allow special formatting inside
                renderer.emit_raw("`")
                for text in f.contents:
                    renderer.emit_raw(cast(Text, text).text.replace("`", "\\`"))
                renderer.emit_raw("`")
            else:
                # If there is special formatting inside, just use HTML
                with renderer.html_mode():
                    with renderer.emit_tag("code"):
                        renderer.emit(f.contents)
        else:
            surround = FORMAT_TYPE_TO_MARKDOWN[f.format_type]
            renderer.emit_raw(surround)
            renderer.emit(f.contents)
            renderer.emit_raw(surround)