import argparse
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
    MarkdownCitationAsFootnotePlugin,
    MarkdownCitationAsHTMLPlugin,
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
    parser = argparse.ArgumentParser()
    parser.add_argument("-olatex", type=str)
    parser.add_argument("-omd", type=str)
    args = parser.parse_args()

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
            MarkdownCitationAsHTMLPlugin(),
            MarkdownFootnotePlugin(),
            MarkdownSectionPlugin(),
            MarkdownFormatPlugin(),
            MarkdownListPlugin(),
            MarkdownUrlPlugin(),
        ]
    )
    r_md.request_postamble_order(
        [
            MarkdownFootnotePlugin._MARKDOWN_FOOTNOTE_POSTAMBLE_ID,
            MarkdownCitationAsHTMLPlugin._BIBLIOGRAPHY_POSTAMBLE_ID,
        ]
    )

    doc_block = r_latex.parse_file(Path("./examples/phdprop.ttxt"))
    rendered_latex = r_latex.render_doc(doc_block)
    if args.olatex:
        with open(args.olatex, "w") as f:
            f.write(rendered_latex)
    else:
        print(rendered_latex)

    doc_block = r_md.parse_file(Path("./examples/phdprop.ttxt"))
    rendered_markdown = r_md.render_doc(doc_block)
    if args.omd:
        with open(args.omd, "w") as f:
            f.write(rendered_markdown)
    else:
        print(rendered_markdown)

    # print(json.dumps(doc_block, indent=4, cls=CustomEncoder))
