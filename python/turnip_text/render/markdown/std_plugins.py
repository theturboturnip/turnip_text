from typing import List

from turnip_text.plugins.cites.markdown import MarkdownCitationPlugin_UncheckedBib
from turnip_text.plugins.doc_structure.markdown import MarkdownStructurePlugin
from turnip_text.plugins.footnote.markdown import MarkdownFootnotePlugin_AtEnd
from turnip_text.plugins.inline_fmt.markdown import MarkdownInlineFormatPlugin
from turnip_text.plugins.list.markdown import MarkdownListPlugin
from turnip_text.plugins.subfile.markdown import MarkdownSubfilePlugin
from turnip_text.plugins.url.markdown import MarkdownUrlPlugin
from turnip_text.render.markdown.renderer import MarkdownPlugin


def STD_MARKDOWN_RENDER_PLUGINS(
    use_chapters: bool,
    indent_list_items: bool = True,
) -> List[MarkdownPlugin]:
    return [
        MarkdownStructurePlugin(use_chapters),
        MarkdownCitationPlugin_UncheckedBib(),
        MarkdownFootnotePlugin_AtEnd(),
        MarkdownListPlugin(indent_list_items),
        MarkdownInlineFormatPlugin(),
        MarkdownUrlPlugin(),
        MarkdownSubfilePlugin(),
    ]
