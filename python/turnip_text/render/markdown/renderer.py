import html
from contextlib import contextmanager
from typing import Iterator, List

from turnip_text import Paragraph, UnescapedText
from turnip_text.doc import DocState, FormatContext
from turnip_text.render import EmitterDispatch, Renderer, Writable


class MarkdownRenderer(Renderer):
    html_mode_stack: List[bool]

    def __init__(
        self,
        doc: DocState,
        fmt: FormatContext,
        handlers: EmitterDispatch,
        write_to: Writable,
        html_mode: bool = True,
    ) -> None:
        super().__init__(doc, fmt, handlers, write_to)
        # Once you're in HTML mode, you can't drop down to Markdown mode again.
        # If they asked for HTML mode only, just make that the first entry in the stack.
        # If they didn't, we start in Markdown mode.
        self.html_mode_stack = [html_mode]

    # def __init__(self, plugins: List[Plugin["MarkdownRenderer"]], html_mode_only: bool=False) -> None:
    #     super().__init__(plugins)
    #     # Once you're in HTML mode, you can't drop down to Markdown mode again.
    #     # If they asked for HTML mode only, just make that the first entry in the stack.
    #     # If they didn't, we start in Markdown mode.
    #     self.html_mode_stack = [html_mode_only]

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
    def in_html_mode(self) -> bool:
        return self.html_mode_stack[-1]

    @contextmanager
    def html_mode(self) -> Iterator[None]:
        self.html_mode_stack.append(True)

        try:
            yield
        finally:
            self.html_mode_stack.pop()

    @contextmanager
    def emit_tag(self, tag: str, props: str | None = None, indent: int = 0):
        if not self.in_html_mode:
            raise RuntimeError("Can't emit_tag without going into HTML mode!")

        if props:
            self.emit_raw(f"<{tag} {props}>")
        else:
            self.emit_raw(f"<{tag}>")

        try:
            if indent:
                with self.indent(indent):
                    self.emit_newline()
                    yield
                    self.emit_newline()
            else:
                yield
        finally:
            self.emit_raw(f"</{tag}>")

    def emit_empty_tag(self, tag: str, props: str | None = None):
        # This is allowed outside of HTML mode because it doesn't contain anything.
        if props:
            self.emit_raw(f"<{tag} {props}></{tag}>")
        else:
            self.emit_raw(f"<{tag}></{tag}>")
