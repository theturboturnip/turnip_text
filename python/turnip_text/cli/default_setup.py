from typing import Any, List, Optional, Tuple

from turnip_text.cli import TurnipTextSetup, TurnipTextSuggestedRenderer
from turnip_text.render import RenderPlugin, RenderSetup
from turnip_text.render.latex.setup import LatexSetup
from turnip_text.render.latex.std_plugins import STD_LATEX_RENDER_PLUGINS
from turnip_text.render.markdown.renderer import HtmlSetup, MarkdownSetup
from turnip_text.render.markdown.std_plugins import STD_MARKDOWN_RENDER_PLUGINS
from turnip_text.render.pandoc import PandocSetup
from turnip_text.render.pandoc.std_plugins import STD_PANDOC_RENDER_PLUGINS


class DefaultTurnipTextSetup(TurnipTextSetup):
    """
    Default setup.

    Supports LaTeX, Markdown, HTML, and Pandoc output.

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
        suggestion: TurnipTextSuggestedRenderer,
        docclass: str = "",
        csl_bib: Optional[str] = None,
        biblatex_bib: Optional[str] = None,
        **kwargs: str,
    ) -> Tuple[RenderSetup, List[RenderPlugin[Any]]]:
        docclass = docclass.casefold()

        if suggestion == TurnipTextSuggestedRenderer.Latex:
            if not biblatex_bib:
                print(
                    "Warning, no BibLaTeX bibliography was supplied so LaTeX will not have citation commands"
                )
            if "book" in docclass:
                return (
                    LatexSetup(),
                    STD_LATEX_RENDER_PLUGINS(
                        doc_class="book",
                        bib=biblatex_bib,
                        bib_output="bibliography.bib",
                    ),
                )
            elif "report" in docclass:
                return (
                    LatexSetup(),
                    STD_LATEX_RENDER_PLUGINS(
                        doc_class="report",
                        bib=biblatex_bib,
                        bib_output="bibliography.bib",
                    ),
                )
            else:
                return (
                    LatexSetup(),
                    STD_LATEX_RENDER_PLUGINS(
                        doc_class="article",
                        bib=biblatex_bib,
                        bib_output="bibliography.bib",
                    ),
                )
        elif suggestion == TurnipTextSuggestedRenderer.Markdown:
            if not csl_bib:
                print(
                    "Warning, no CSL bibliography was supplied so Markdown will not have citation commands"
                )
            return (
                MarkdownSetup(),
                STD_MARKDOWN_RENDER_PLUGINS(
                    use_chapters=("book" in docclass) or ("report" in docclass),
                    bib=csl_bib,
                ),
            )
        elif suggestion == TurnipTextSuggestedRenderer.HTML:
            if not csl_bib:
                print(
                    "Warning, no CSL bibliography was supplied so HTML will not have citation commands"
                )
            return (
                HtmlSetup(),
                STD_MARKDOWN_RENDER_PLUGINS(
                    use_chapters=("book" in docclass) or ("report" in docclass),
                    bib=csl_bib,
                ),
            )
        elif suggestion == TurnipTextSuggestedRenderer.Pandoc:
            if not csl_bib:
                print(
                    "Warning, no CSL bibliography was supplied so Pandoc will not have citation commands"
                )
            return (
                PandocSetup(),
                STD_PANDOC_RENDER_PLUGINS(
                    bib=csl_bib,
                ),
            )

        return super().generate_setup(suggestion, **kwargs)
