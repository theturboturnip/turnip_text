from contextlib import contextmanager
from typing import Any, Iterator

from turnip_text import UnescapedText
from turnip_text.render import Renderer


class LatexRenderer(Renderer):
    def emit_unescapedtext(self, t: UnescapedText) -> None:
        # note - right now this assumes we're using a unicode-compatible setup and thus don't need to escape unicode characters.
        # note - order is important here because the subsitutions may introduce more special characters.
        # e.g. if the backslash replacement applied later, the backslash from escaping "%" would be replaced with \textbackslash
        ascii_map = {
            "\\": "\\textbackslash{}",
            "%": "\%",
            "$": "\$",
            "{": "\{",
            "}": "\}",
            "_": "\_",
            "#": "\#",
            "&": "\&",
            "~": "\~{}",
        }
        data = t.text
        for c, replace_with in ascii_map.items():
            data = data.replace(c, replace_with)
        self.emit_raw(data)

    def emit_macro(self, name: str) -> None:
        self.emit_raw(f"\\{name}")

    def emit_sqr_bracketed(self, *args: Any) -> None:
        self.emit_raw("[")
        self.emit(*args)
        self.emit_raw("]")

    def emit_braced(self, *args: Any) -> None:
        self.emit_raw("{")
        self.emit(*args)
        self.emit_raw("}")

    @contextmanager
    def emit_env(self, name: str, indent: int = 4) -> Iterator[None]:
        self.emit_macro("begin")
        self.emit_braced(name)
        self.push_indent(indent)
        self.emit_break_sentence()

        try:
            yield
        finally:
            self.pop_indent(indent)
            self.emit_break_sentence()
            self.emit_macro("end")
            self.emit_braced(name)
