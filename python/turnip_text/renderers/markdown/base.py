from dataclasses import dataclass
from typing import List
from turnip_text.renderers import Renderer, RendererPlugin
from turnip_text import Inline, UnescapedText

class MarkdownRenderer(Renderer):
    PARAGRAPH_SEP = "\n\n"
    SENTENCE_SEP = "\n"

    def render_unescapedtext(self, t: UnescapedText) -> str:
        # note - right now this assumes we're using a unicode-compatible setup and thus don't need to escape unicode characters.
        # note - order is important here because the subsitutions may introduce more special characters.
        # e.g. if the backslash replacement applied later, the backslash from escaping "%" would be replaced with \textbackslash

        # TODO - some of these are overzealous, e.g. (), because in most contexts they're interpreted as normal text.
        # context sensitive escaping?
        # https://www.markdownguide.org/basic-syntax/
        special_chars=r"\\`*_{}[]<>()#+-.!|"

        raise NotImplementedError("Need to replace chars")