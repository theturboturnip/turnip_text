from turnip_text.plugins.subfile import SubfileEnvPlugin
from turnip_text.render.markdown.renderer import MarkdownPlugin


class MarkdownSubfilePlugin(MarkdownPlugin, SubfileEnvPlugin):
    pass
