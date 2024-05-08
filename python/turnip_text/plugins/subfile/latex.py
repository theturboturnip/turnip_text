from turnip_text.plugins.subfile import SubfileEnvPlugin
from turnip_text.render.latex.setup import LatexPlugin


class LatexSubfilePlugin(LatexPlugin, SubfileEnvPlugin):
    pass
