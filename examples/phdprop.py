import json
from pathlib import Path

from turnip_text import *
from turnip_text.renderers.latex import LatexRenderer
from turnip_text.renderers.latex.plugins import (
    LatexCitationPlugin,
    LatexFootnotePlugin,
    LatexFormatPlugin,
    LatexListPlugin,
    LatexSectionPlugin,
    LatexUrlPlugin,
)
from turnip_text.renderers.markdown.base import MarkdownRenderer
from turnip_text.renderers.markdown.plugins import (
    MarkdownCitationPlugin,
    MarkdownFootnotePlugin,
    MarkdownFormatPlugin,
    MarkdownListPlugin,
    MarkdownSectionPlugin,
    MarkdownUrlPlugin,
)


class CustomEncoder(json.JSONEncoder):
    def default(self, o):
        if isinstance(o, (BlockScope, InlineScope, Paragraph, Sentence)):
            return list(o)
        if isinstance(o, UnescapedText):
            return o.text
        if hasattr(o, "__dict__"):
            d = vars(o)
            d["str"] = str(o)
            return d
        return str(o)


if __name__ == "__main__":
    r_latex = LatexRenderer(
        [
            LatexCitationPlugin(),
            LatexFootnotePlugin(),
            LatexSectionPlugin(),
            LatexFormatPlugin(),
            LatexListPlugin(),
            LatexUrlPlugin(),
        ]
    )
    r_md = MarkdownRenderer(
        [
            MarkdownCitationPlugin(),
            MarkdownFootnotePlugin(),
            MarkdownSectionPlugin(),
            MarkdownFormatPlugin(),
            MarkdownListPlugin(),
            MarkdownUrlPlugin(),
        ]
    )

    doc_block = r_latex.parse_file(Path("./examples/phdprop.ttxt"))
    print(r_latex.render_doc(doc_block))

    # r.load_cites("phdprop.bibtex")
    doc_block = r_md.parse_file(Path("./examples/phdprop.ttxt"))
    print(r_md.render_doc(doc_block))

    # print(json.dumps(doc_block, indent=4, cls=CustomEncoder))
