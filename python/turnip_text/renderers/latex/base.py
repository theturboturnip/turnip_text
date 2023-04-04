from dataclasses import dataclass
from typing import List

from turnip_text import Inline, UnescapedText
from turnip_text.renderers import Renderer, RendererPlugin


@dataclass(frozen=True)
class RawLatex(Inline):
    text: str


class LatexRenderer(Renderer):
    PARAGRAPH_SEP = "\n\n"
    SENTENCE_SEP = "\n"

    def __init__(self, plugins: List[RendererPlugin]) -> None:
        super().__init__(plugins)

        self.inline_handlers.push_association(
            (RawLatex, lambda _, raw: self.render_raw_latex(raw))
        )

    def render_raw_latex(self, r: RawLatex) -> str:
        return r.text

    def render_unescapedtext(self, t: UnescapedText) -> str:
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
        return data
