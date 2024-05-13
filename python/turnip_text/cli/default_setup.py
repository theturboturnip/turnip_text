from typing import Optional

from turnip_text.cli import GeneratedSetup, TurnipTextSetup
from turnip_text.render.latex.setup import LatexSetup
from turnip_text.render.latex.std_plugins import STD_LATEX_RENDER_PLUGINS
from turnip_text.render.markdown.renderer import HtmlSetup, MarkdownSetup
from turnip_text.render.markdown.std_plugins import STD_MARKDOWN_RENDER_PLUGINS
from turnip_text.render.pandoc import PandocSetup, recommend_pandoc_format_ext
from turnip_text.render.pandoc.std_plugins import STD_PANDOC_RENDER_PLUGINS


class DefaultTurnipTextSetup(TurnipTextSetup):
    """
    Default setup.

    Supported formats:
    - "latex" for LaTeX
    - "markdown" for Markdown
    - "html" for HTML
    - "pandoc-{format}" for Pandoc output to the given format 

    Accepts three keyword arguments:
    - `docclass:(article|book|report)`
        - For LaTeX, set the documentclass
        - For Markdown and HTML, 'book' and 'report' cause them to treat first level headings as chapters, not sections.
        - No effect on Pandoc
    - `csl_bib:{path}`
        - Set the path to a CSL JSON bibliography used in Markdown, HTML, Pandoc
    - `biblatex_bib:{path}` - set the path to a BibLaTeX bibliography
        - Set the path to a BibLaTeX bibliography used in LaTeX
    """

    def generate_setup(
        self,
        input_stem: str,
        requested_format: str,
        docclass: str = "",
        csl_bib: Optional[str] = None,
        biblatex_bib: Optional[str] = None,
        **kwargs: str,
    ) -> GeneratedSetup:
        requested_format = requested_format.casefold()
        docclass = docclass.casefold()

        if requested_format == "latex":
            if not biblatex_bib:
                print(
                    "Warning, no BibLaTeX bibliography was supplied so LaTeX will not have citation commands"
                )
            if "book" in docclass:
                return GeneratedSetup(
                    LatexSetup(),
                    STD_LATEX_RENDER_PLUGINS(
                        doc_class="book",
                        bib=biblatex_bib,
                        bib_output=f"{input_stem}.bib",
                    ),
                    f"{input_stem}.tex"
                )
            elif "report" in docclass:
                return GeneratedSetup(
                    LatexSetup(),
                    STD_LATEX_RENDER_PLUGINS(
                        doc_class="report",
                        bib=biblatex_bib,
                        bib_output=f"{input_stem}.bib",
                    ),
                    f"{input_stem}.tex"
                )
            else:
                return GeneratedSetup(
                    LatexSetup(),
                    STD_LATEX_RENDER_PLUGINS(
                        doc_class="article",
                        bib=biblatex_bib,
                        bib_output=f"{input_stem}.bib",
                    ),
                    f"{input_stem}.tex"
                )
        elif requested_format == "md":
            if not csl_bib:
                print(
                    "Warning, no CSL bibliography was supplied so Markdown will not have citation commands"
                )
            return GeneratedSetup(
                MarkdownSetup(),
                STD_MARKDOWN_RENDER_PLUGINS(
                    use_chapters=("book" in docclass) or ("report" in docclass),
                    bib=csl_bib,
                ),
                    f"{input_stem}.md"
            )
        elif requested_format == "html":
            if not csl_bib:
                print(
                    "Warning, no CSL bibliography was supplied so HTML will not have citation commands"
                )
            return GeneratedSetup(
                HtmlSetup(),
                STD_MARKDOWN_RENDER_PLUGINS(
                    use_chapters=("book" in docclass) or ("report" in docclass),
                    bib=csl_bib,
                ),
                    f"{input_stem}.html"
            )
        elif requested_format.startswith("pandoc-"):
            if not csl_bib:
                print(
                    "Warning, no CSL bibliography was supplied so Pandoc will not have citation commands"
                )
            pandoc_format = requested_format.removeprefix("pandoc-")
            return GeneratedSetup(
                PandocSetup(pandoc_format),
                STD_PANDOC_RENDER_PLUGINS(
                    bib=csl_bib,
                ),
                f"{input_stem}.{recommend_pandoc_format_ext(pandoc_format)}"
            )

        return super().generate_setup(input_stem, requested_format, **kwargs)
