import argparse
import io
import json
from io import StringIO
from pathlib import Path
from typing import List, Sequence, Type, TypeVar

from turnip_text import *
from turnip_text.doc import DocSetup
from turnip_text.doc.std_plugins import STD_DOC_PLUGINS
from turnip_text.render import Renderer
from turnip_text.render.latex.renderer import LatexSetup
from turnip_text.render.latex.std_plugins import STD_LATEX_RENDER_PLUGINS
from turnip_text.render.manual_numbering import LOWER_ROMAN_NUMBERING
from turnip_text.render.markdown.renderer import (
    HtmlSetup,
    MarkdownCounterFormatting,
    MarkdownSetup,
)
from turnip_text.render.markdown.std_plugins import STD_MARKDOWN_RENDER_PLUGINS
from turnip_text.system import parse_and_emit


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

    rendered_latex = parse_and_emit(
        InsertedFile.from_path("./examples/phdprop.ttext"),
        DocSetup(STD_DOC_PLUGINS()),
        LatexSetup(STD_LATEX_RENDER_PLUGINS(use_chapters=False)),
        write_to=io.StringIO(),
    )
    if args.olatex:
        with open(args.olatex, "w") as f:
            f.write(rendered_latex.getvalue())
    else:
        print(rendered_latex.getvalue())

    rendered_markdown = parse_and_emit(
        InsertedFile.from_path("./examples/phdprop.ttext"),
        DocSetup(STD_DOC_PLUGINS()),
        MarkdownSetup(STD_MARKDOWN_RENDER_PLUGINS(use_chapters=False)),
        write_to=io.StringIO(),
    )
    if args.omd:
        with open(args.omd, "w") as f:
            f.write(rendered_markdown.getvalue())
    else:
        print(rendered_markdown.getvalue())

    rendered_html = parse_and_emit(
        InsertedFile.from_path("./examples/phdprop.ttext"),
        DocSetup(STD_DOC_PLUGINS()),
        HtmlSetup(
            STD_MARKDOWN_RENDER_PLUGINS(use_chapters=False),
            requested_counter_formatting={
                "footnote": MarkdownCounterFormatting("", style=LOWER_ROMAN_NUMBERING)
            },
            requested_counter_links=[("h1", "footnote")],
        ),
        write_to=io.StringIO(),
    )
    if args.ohtml:
        with open(args.ohtml, "w") as f:
            f.write(rendered_html.getvalue())
    else:
        print(rendered_html.getvalue())
