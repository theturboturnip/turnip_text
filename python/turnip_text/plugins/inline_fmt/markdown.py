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

# TODO some formatting breaks when directly following text e.g. "AXI**\[TODO: cite\]** and PCIe**\[TODO: cite\]**" bold the " and PCIe" not the [TODO bits]

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
        elif f.format_type in [InlineFormattingType.Bold, InlineFormattingType.Italic, InlineFormattingType.Emph, InlineFormattingType.Strong]:
            # If preceded by a space, we're fine to use markdown
            # If in plain-text mode, we can use this too.
            if renderer.peek(1).isspace() or (not renderer.html_allowed):
                surround = FORMAT_TYPE_TO_MARKDOWN[f.format_type]
                renderer.emit_raw(surround)
                renderer.emit(f.contents)
                renderer.emit_raw(surround)
            else:
                # Otherwise doing e.g. "AXI**\[blah\]** " can have problems, so drop down to HTML
                with renderer.html_mode():
                    with renderer.emit_tag(FORMAT_TYPE_TO_HTML[f.format_type]):
                        renderer.emit(f.contents)
        elif f.format_type == InlineFormattingType.Underline:
            # Have to go into html mode for this
            if renderer.html_allowed:
                with renderer.html_mode():
                    with renderer.emit_tag("u"):
                        renderer.emit(f.contents)
            else:
                # best effort, just render contents
                renderer.emit(f.contents)
        elif f.format_type == InlineFormattingType.Mono:
            if all(isinstance(i, Text) for i in f.contents) or (not renderer.html_allowed):
                # Markdown ` doesn't allow special formatting inside, but plain text does
                renderer.emit_raw("`")
                for text in f.contents:
                    renderer.emit_raw(cast(Text, text).text.replace("`", "\\`"))
                renderer.emit_raw("`")
            else:
                # If there is special formatting inside, just use HTML
                with renderer.html_mode():
                    with renderer.emit_tag("code"):
                        renderer.emit(f.contents)
