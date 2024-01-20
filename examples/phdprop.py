import argparse
import json
from io import StringIO
from pathlib import Path
from typing import List, Sequence, Type, TypeVar

from turnip_text import *
from turnip_text.doc import DocPlugin, mutate_pass, parse_pass
from turnip_text.doc.std_plugins import STD_DOC_PLUGINS
from turnip_text.render import Renderer, RenderPlugin
from turnip_text.render.counters import CounterSet
from turnip_text.render.latex.renderer import LatexRenderer
from turnip_text.render.latex.std_plugins import STD_LATEX_RENDER_PLUGINS
from turnip_text.render.markdown.renderer import MarkdownRenderer
from turnip_text.render.markdown.std_plugins import STD_MARKDOWN_RENDER_PLUGINS

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


TRenderer = TypeVar("TRenderer", bound="Renderer", contravariant=True)


def parse_and_render(
    p: Path,
    doc_plugins: List[DocPlugin],
    r: Type[TRenderer],
    renderer_plugins: List[RenderPlugin[TRenderer]],
    **renderer_kwargs,
) -> StringIO:
    doc, fmt, doc_toplevel = parse_pass(p, doc_plugins)
    mutators: List[DocMutator] = doc_plugins + renderer_plugins  # type: ignore
    document = mutate_pass(doc, fmt, doc_toplevel, mutators)
    return r.render(renderer_plugins, document, **renderer_kwargs)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("-olatex", type=str)
    parser.add_argument("-omd", type=str)
    parser.add_argument("-ohtml", type=str)
    args = parser.parse_args()

    rendered_latex = parse_and_render(
        Path("./examples/phdprop.ttext"),
        STD_DOC_PLUGINS(),
        LatexRenderer,
        STD_LATEX_RENDER_PLUGINS(use_chapters=False),
    )
    if args.olatex:
        with open(args.olatex, "w") as f:
            f.write(rendered_latex.getvalue())
    else:
        print(rendered_latex.getvalue())

    rendered_markdown = parse_and_render(
        Path("./examples/phdprop.ttext"),
        STD_DOC_PLUGINS(),
        MarkdownRenderer,
        STD_MARKDOWN_RENDER_PLUGINS(use_chapters=False),
    )
    if args.omd:
        with open(args.omd, "w") as f:
            f.write(rendered_markdown.getvalue())
    else:
        print(rendered_markdown.getvalue())

    rendered_html = parse_and_render(
        Path("./examples/phdprop.ttext"),
        STD_DOC_PLUGINS(),
        MarkdownRenderer,
        STD_MARKDOWN_RENDER_PLUGINS(use_chapters=False),
        html_mode=True,
    )
    if args.ohtml:
        with open(args.ohtml, "w") as f:
            f.write(rendered_html.getvalue())
    else:
        print(rendered_html.getvalue())

    # doc_block = r_html.parse_file(Path("./examples/phdprop.ttxt"))
    # rendered_html = r_html.render_doc(doc_block)
    # if args.ohtml:
    #     with open(args.ohtml, "w") as f:
    #         f.write(rendered_html.getvalue())
    # else:
    #     print(rendered_html.getvalue())

    # print(json.dumps(doc_block, indent=4, cls=CustomEncoder))
