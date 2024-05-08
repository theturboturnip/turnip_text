from typing import List

from turnip_text.plugins.cites.latex import LatexBiblatexPlugin_Unchecked
from turnip_text.plugins.doc_structure.latex import LatexStructurePlugin_Article
from turnip_text.plugins.footnote.latex import LatexFootnotePlugin
from turnip_text.plugins.inline_fmt.latex import LatexInlineFormatPlugin
from turnip_text.plugins.list.latex import LatexListPlugin
from turnip_text.plugins.subfile.latex import LatexSubfilePlugin
from turnip_text.plugins.url.latex import LatexUrlPlugin
from turnip_text.render.latex.setup import LatexPlugin


def STD_LATEX_ARTICLE_RENDER_PLUGINS(
    use_chapters: bool,
    indent_list_items: bool = True,
) -> List[LatexPlugin]:
    return [
        LatexStructurePlugin_Article(use_chapters),
        LatexBiblatexPlugin_Unchecked(),
        LatexFootnotePlugin(),
        LatexListPlugin(indent_list_items),
        LatexInlineFormatPlugin(),
        LatexUrlPlugin(),
        LatexSubfilePlugin(),
    ]
