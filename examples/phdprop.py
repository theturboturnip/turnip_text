import argparse
import json
from pathlib import Path

from turnip_text import *
from turnip_text.build_system import (
    InMemoryBuildSystem,
    SimpleBuildSystem,
    SplitBuildSystem,
)
from turnip_text.render.latex.renderer import LatexCounterStyle
from turnip_text.render.latex.setup import LatexSetup
from turnip_text.render.latex.std_plugins import STD_LATEX_ARTICLE_RENDER_PLUGINS
from turnip_text.render.manual_numbering import SimpleCounterFormat
from turnip_text.render.markdown.renderer import (
    HtmlSetup,
    MarkdownCounterStyle,
    MarkdownSetup,
)
from turnip_text.render.markdown.std_plugins import STD_MARKDOWN_RENDER_PLUGINS
from turnip_text.system import parse_and_emit


class CustomEncoder(json.JSONEncoder):
    def default(self, o):
        if isinstance(o, (BlockScope, InlineScope, Paragraph, Sentence)):
            return list(o)
        if isinstance(o, Text):
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

    real_build_sys = SimpleBuildSystem(
        project_dir=Path("./examples"), output_dir=Path("./examples/output/")
    )
    in_memory_build_sys = InMemoryBuildSystem(input_files={})

    # Parse into a BuildSystem which always takes input from the filesystem, but either writes out to an in-memory filesystem or the real filesystem depending on the requested output argument.
    parse_and_emit(
        SplitBuildSystem(
            input_build_sys=real_build_sys,
            output_build_sys=(real_build_sys if args.olatex else in_memory_build_sys),
        ),
        "phdprop.ttext",
        args.olatex,
        LatexSetup(
            standalone=False,
            latex_counter_format_override={
                "section": SimpleCounterFormat(
                    "section", LatexCounterStyle.RomanUpper, postfix_for_child="-"
                ),
            },
        ),
        STD_LATEX_ARTICLE_RENDER_PLUGINS(
            use_chapters=False,
            bib="phdprop_bib_biblatex.bib",
            bib_output="example.bib",
        ),
    )

    # Parse into a BuildSystem which always takes input from the filesystem, but either writes out to an in-memory filesystem or the real filesystem depending on the requested output argument.
    parse_and_emit(
        SplitBuildSystem(
            input_build_sys=real_build_sys,
            output_build_sys=(real_build_sys if args.omd else in_memory_build_sys),
        ),
        "phdprop.ttext",
        args.omd,
        MarkdownSetup(),
        STD_MARKDOWN_RENDER_PLUGINS(
            use_chapters=False,
            bib="phdprop_bib_csl.json",
        ),
    )

    # Parse into a BuildSystem which always takes input from the filesystem, but either writes out to an in-memory filesystem or the real filesystem depending on the requested output argument.
    parse_and_emit(
        SplitBuildSystem(
            input_build_sys=real_build_sys,
            output_build_sys=(real_build_sys if args.ohtml else in_memory_build_sys),
        ),
        "phdprop.ttext",
        args.ohtml,
        HtmlSetup(
            requested_counter_formatting={
                "footnote": SimpleCounterFormat(
                    "", style=MarkdownCounterStyle.RomanLower
                )
            },
            requested_counter_links=[("h1", "footnote")],
        ),
        STD_MARKDOWN_RENDER_PLUGINS(
            use_chapters=False,
            bib="phdprop_bib_csl.json",
        ),
    )

    for (
        output_file_name,
        output_file_contents,
    ) in in_memory_build_sys.get_outputs().items():
        print("=====================================")
        print(output_file_name)
        print("=====================================")
        print(output_file_contents.decode("utf-8"))
