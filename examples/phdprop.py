import argparse
import json
from pathlib import Path

from turnip_text import *
from turnip_text.doc import parse
from turnip_text.doc.std_plugins import STD_DOC_PLUGINS
from turnip_text.render.counters import (
    ARABIC_NUMBERING,
    BasicCounter,
    Counter,
    CounterSet,
)
from turnip_text.render.latex.renderer import LatexRenderer
from turnip_text.render.latex.std_plugins import STD_LATEX_RENDER_PLUGINS

# from turnip_text.render.markdown.base import MarkdownRenderer
# from turnip_text.render.markdown.plugins import (
#     MarkdownCitationAsFootnotePlugin,
#     MarkdownCitationAsHTMLPlugin,
#     MarkdownFootnotePlugin,
#     MarkdownFormatPlugin,
#     MarkdownListPlugin,
#     MarkdownSectionPlugin,
#     MarkdownUrlPlugin,
# )


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
    parser.add_argument("-ohtml", type=str)
    args = parser.parse_args()

    # r_md = MarkdownRenderer(
    #     [
    #         MarkdownCitationAsHTMLPlugin(),
    #         MarkdownFootnotePlugin(),
    #         MarkdownSectionPlugin(),
    #         MarkdownFormatPlugin(),
    #         MarkdownListPlugin(),
    #         MarkdownUrlPlugin(),
    #     ]
    # )
    # r_md.request_postamble_order(
    #     [
    #         MarkdownFootnotePlugin._MARKDOWN_FOOTNOTE_POSTAMBLE_ID,
    #         MarkdownCitationAsHTMLPlugin._BIBLIOGRAPHY_POSTAMBLE_ID,
    #     ]
    # )

    # r_html = MarkdownRenderer(
    #     [
    #         MarkdownCitationAsHTMLPlugin(),
    #         MarkdownFootnotePlugin(),
    #         MarkdownSectionPlugin(),
    #         MarkdownFormatPlugin(),
    #         MarkdownListPlugin(),
    #         MarkdownUrlPlugin(),
    #     ],
    #     html_mode_only=True,
    # )
    # r_html.request_postamble_order(
    #     [
    #         MarkdownFootnotePlugin._MARKDOWN_FOOTNOTE_POSTAMBLE_ID,
    #         MarkdownCitationAsHTMLPlugin._BIBLIOGRAPHY_POSTAMBLE_ID,
    #     ]
    # )

    doc = parse(Path("./examples/phdprop.ttxt"), STD_DOC_PLUGINS())

    latex_counters = CounterSet(
        [
            BasicCounter("h1", "Section", ARABIC_NUMBERING, [
                BasicCounter("h2", "Subsection", ARABIC_NUMBERING)
            ])
        ]
    )
    rendered_latex = LatexRenderer.render(STD_LATEX_RENDER_PLUGINS(use_chapters=False), doc)
    if args.olatex:
        with open(args.olatex, "w") as f:
            f.write(rendered_latex.getvalue())
    else:
        print(rendered_latex.getvalue())

    # doc_block = r_md.parse_file(Path("./examples/phdprop.ttxt"))
    # rendered_markdown = r_md.render_doc(doc_block)
    # if args.omd:
    #     with open(args.omd, "w") as f:
    #         f.write(rendered_markdown.getvalue())
    # else:
    #     print(rendered_markdown.getvalue())

    # doc_block = r_html.parse_file(Path("./examples/phdprop.ttxt"))
    # rendered_html = r_html.render_doc(doc_block)
    # if args.ohtml:
    #     with open(args.ohtml, "w") as f:
    #         f.write(rendered_html.getvalue())
    # else:
    #     print(rendered_html.getvalue())

    # print(json.dumps(doc_block, indent=4, cls=CustomEncoder))
