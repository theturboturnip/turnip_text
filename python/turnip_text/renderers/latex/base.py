from dataclasses import dataclass
from typing import List
from turnip_text.renderers import Renderer, RendererPlugin
from turnip_text import Inline, UnescapedText

@dataclass(frozen=True)
class RawLatex(Inline):
    text: str

class LatexRenderer(Renderer):
    PARAGRAPH_SEP = "\n\n"
    SENTENCE_SEP = "\n"

    def __init__(self, plugins: List[RendererPlugin]) -> None:
        super().__init__(plugins)
        
        self.inline_handlers.push_association((RawLatex, lambda _, raw: self.render_raw_latex(raw)))
    
    def render_raw_latex(self, r: RawLatex) -> str:
        return r.text

    def render_unescapedtext(self, t: UnescapedText) -> str:
        ### TODO!
        return t.text