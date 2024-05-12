from typing import List, Optional

from turnip_text.build_system import InputRelPath
from turnip_text.plugins.cites.pandoc import PandocCitationPlugin
from turnip_text.plugins.doc_structure.pandoc import PandocStructurePlugin
from turnip_text.plugins.footnote.pandoc import PandocFootnotePlugin
from turnip_text.plugins.inline_fmt.pandoc import PandocInlineFormatPlugin
from turnip_text.plugins.list.pandoc import PandocListPlugin
from turnip_text.plugins.primitives.pandoc import PandocPrimitivesPlugin
from turnip_text.plugins.url.pandoc import PandocUrlPlugin
from turnip_text.render.pandoc import PandocPlugin


def STD_PANDOC_RENDER_PLUGINS(
    bib: Optional[InputRelPath] = None,
) -> List[PandocPlugin]:
    plugins = [
        PandocStructurePlugin(),
        PandocFootnotePlugin(),
        PandocListPlugin(),
        PandocInlineFormatPlugin(),
        PandocUrlPlugin(),
        PandocPrimitivesPlugin(),
    ]
    if bib:
        plugins.append(PandocCitationPlugin(csl_json_path=bib))
    return plugins
