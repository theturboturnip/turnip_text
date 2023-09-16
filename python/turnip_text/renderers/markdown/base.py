import html
from contextlib import contextmanager
from dataclasses import dataclass
from typing import List

from turnip_text import Block, BlockScope, Inline, Paragraph, UnescapedText
from turnip_text.renderers import Plugin, Renderer


@dataclass(frozen=True)
class RawMarkdown(Inline):
    md: str


class MarkdownRenderer(Renderer):
    html_mode_stack: List[bool]

    def __init__(self, plugins: List[Plugin["MarkdownRenderer"]], start_in_html_mode: bool=False) -> None:
        super().__init__(plugins)
        self.emit_dispatch.add_custom_inline(
            RawMarkdown, lambda r, ctx, raw: r._emit_raw_markdown(raw)
        )
        # We're initially in Markdown mode, not HTML mode
        self.html_mode_stack = [start_in_html_mode]

    def _emit_raw_markdown(self, r: RawMarkdown) -> None:
        self.emit_raw(r.md)

    def emit_unescapedtext(self, t: UnescapedText) -> None:
        if self.in_html_mode:
            self.emit_raw(html.escape(t.text))
        else:
            # note - right now this assumes we're using a unicode-compatible setup and thus don't need to escape unicode characters.
            # note - order is important here because the subsitutions may introduce more special characters.
            # e.g. if the backslash replacement applied later, the backslash from escaping "(" would be escaped as well

            # TODO - some of these are overzealous, e.g. () and -, because in most contexts they're interpreted as normal text.
            # context sensitive escaping?
            # https://www.markdownguide.org/basic-syntax/
            special_chars = r"\\`*_{}[]<>()#+-.!|"

            data = t.text
            for char in special_chars:
                data = data.replace(char, "\\" + char)
            self.emit_raw(data)

    def emit_paragraph(self, p: Paragraph) -> None:
        if self.in_html_mode:
            self.emit_raw("<p>")
            self.emit_newline()
            with self.indent(4):
                super().emit_paragraph(p)
                # emit_paragraph already ends with a newline
            self.emit_raw("</p>")
        else:
            super().emit_paragraph(p)

    # TODO do this for blocks instead of paragraphs?
    # def emit_block(self, b: Block) -> None:
    #     if self.in_html_mode:
    #         self.emit_raw("<p>")
    #         with self.indent(4):
    #             self.emit_line_break()
    #             super().emit_block(b)
    #         self.emit_line_break()
    #         self.emit_raw("</p>")
    #     else:
    #         super().emit_block(b)

    @property
    def in_html_mode(self):
        return self.html_mode_stack[-1]
    
    @contextmanager
    def html_mode(self):
        self.html_mode_stack.append(True)

        try:
            yield
        finally:
            self.html_mode_stack.pop()
