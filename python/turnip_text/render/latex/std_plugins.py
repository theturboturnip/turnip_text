from typing import List, Optional

from turnip_text.build_system import OutputRelativePath, ProjectRelativePath
from turnip_text.plugins.cites.latex import LatexBiblatexCitationPlugin
from turnip_text.plugins.doc_structure.latex import (
    BasicLatexDocClass,
    LatexDocumentClassPlugin_Basic,
    StartLatexHeader,
)
from turnip_text.plugins.footnote.latex import LatexFootnotePlugin
from turnip_text.plugins.inline_fmt.latex import LatexInlineFormatPlugin
from turnip_text.plugins.list.latex import LatexListPlugin
from turnip_text.plugins.subfile.latex import LatexSubfilePlugin
from turnip_text.plugins.url.latex import LatexUrlPlugin
from turnip_text.render.latex.setup import LatexPlugin


def STD_LATEX_ARTICLE_RENDER_PLUGINS(
    h1: StartLatexHeader = "section",
    doc_class: BasicLatexDocClass = "article",
    indent_list_items: bool = True,
    bib: Optional[ProjectRelativePath] = None,
    bib_output: Optional[OutputRelativePath] = None,
) -> List[LatexPlugin]:
    plugins = [
        LatexDocumentClassPlugin_Basic(h1, doc_class=doc_class),
        LatexFootnotePlugin(),
        LatexListPlugin(indent_list_items),
        LatexInlineFormatPlugin(),
        LatexUrlPlugin(),
        LatexSubfilePlugin(),
    ]
    if bib:
        plugins.append(LatexBiblatexCitationPlugin(bib, bib_output))
    elif bib_output:
        raise ValueError("Can't set bib_output when bib is not set")
    return plugins
