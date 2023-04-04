from dataclasses import dataclass
from typing import List
from turnip_text.renderers import Renderer, RendererPlugin
from turnip_text import Inline, UnescapedText

@dataclass(frozen=True)
class RawMarkdown(Inline):
    md: str

class MarkdownRenderer(Renderer):
    PARAGRAPH_SEP = "\n\n"
    SENTENCE_SEP = "\n"

    def __init__(self, plugins: List['RendererPlugin']) -> None:
        super().__init__(plugins)
        self.inline_handlers.push_association((RawMarkdown, lambda _, raw: self._render_raw_markdown(raw)))

    def _render_raw_markdown(self, r: RawMarkdown) -> str:
        return r.md

    def render_unescapedtext(self, t: UnescapedText) -> str:
        # note - right now this assumes we're using a unicode-compatible setup and thus don't need to escape unicode characters.
        # note - order is important here because the subsitutions may introduce more special characters.
        # e.g. if the backslash replacement applied later, the backslash from escaping "(" would be escaped as well

        # TODO - some of these are overzealous, e.g. () and -, because in most contexts they're interpreted as normal text.
        # context sensitive escaping?
        # https://www.markdownguide.org/basic-syntax/
        special_chars=r"\\`*_{}[]<>()#+-.!|"

        data = t.text
        for char in special_chars:
            data = data.replace(char, "\\"+char)
        return data