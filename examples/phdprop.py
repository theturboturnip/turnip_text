import argparse
import json
from pathlib import Path

from turnip_text import *
from turnip_text.build_system import InMemoryBuildSystem, SimpleBuildSystem
from turnip_text.render.latex.backrefs import LatexBackrefMethod
from turnip_text.render.latex.renderer import LatexCounterStyle
from turnip_text.render.latex.setup import LatexSetup
from turnip_text.render.latex.std_plugins import STD_LATEX_ARTICLE_RENDER_PLUGINS
from turnip_text.render.manual_numbering import SimpleCounterFormat, SimpleCounterStyle
from turnip_text.render.markdown.renderer import HtmlSetup, MarkdownSetup
from turnip_text.render.markdown.std_plugins import STD_MARKDOWN_RENDER_PLUGINS
from turnip_text.render.pandoc import PandocSetup
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
    parser.add_argument("-odocx", type=str)
    args = parser.parse_args()

    real_build_sys = SimpleBuildSystem(
        project_dir=Path("./examples"), output_dir=Path("./examples/output/")
    )
    in_memory_build_sys = InMemoryBuildSystem(input_files={})

    # LaTeX
    parse_and_emit(
        real_build_sys,
        "phdprop.ttext",
        args.olatex,
        LatexSetup(
            standalone=False,
            latex_counter_format_override={
                "section": SimpleCounterFormat(
                    "section", LatexCounterStyle.RomanUpper, postfix_for_child="-"
                ),
                "appendix": SimpleCounterFormat("Appx.", LatexCounterStyle.AlphUpper),
            },
            # You can exclude certain supported backref methods if you want
            legal_backref_methods=[
                LatexBackrefMethod.Cleveref,
                LatexBackrefMethod.Hyperlink,
                LatexBackrefMethod.PageRef,
                LatexBackrefMethod.ManualRef,
            ],
        ),
        STD_LATEX_ARTICLE_RENDER_PLUGINS(
            h1="section",
            doc_class="article",
            bib="phdprop_bib_biblatex.bib",
            bib_output="example.bib",
        ),
    )

    # Markdown
    parse_and_emit(
        real_build_sys,
        "phdprop.ttext",
        args.omd,
        MarkdownSetup(),
        STD_MARKDOWN_RENDER_PLUGINS(
            use_chapters=False,
            bib="phdprop_bib_csl.json",
        ),
    )

    # HTML
    parse_and_emit(
        real_build_sys,
        "phdprop.ttext",
        args.ohtml,
        HtmlSetup(
            requested_counter_formatting={
                "footnote": SimpleCounterFormat("", style=SimpleCounterStyle.RomanLower)
            },
            requested_counter_links=[("h1", "footnote")],
        ),
        STD_MARKDOWN_RENDER_PLUGINS(
            use_chapters=False,
            bib="phdprop_bib_csl.json",
        ),
    )

    # Pandoc (autodetects from output, in this case DOCX)
    parse_and_emit(real_build_sys, "phdprop.ttext", "document.docx", PandocSetup(), [])

    for (
        output_file_name,
        output_file_contents,
    ) in in_memory_build_sys.get_outputs().items():
        print("=====================================")
        print(output_file_name)
        print("=====================================")
        print(output_file_contents.decode("utf-8"))
