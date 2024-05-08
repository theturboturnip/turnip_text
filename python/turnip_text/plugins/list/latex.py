from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.list import (
    DisplayList,
    DisplayListItem,
    DisplayListType,
    ListEnvPlugin,
)
from turnip_text.render.latex.renderer import LatexRenderer
from turnip_text.render.latex.setup import LatexPlugin, LatexSetup


class LatexListPlugin(LatexPlugin, ListEnvPlugin):
    indent_list_items: bool = True

    def __init__(self, indent_list_items: bool = True):
        self.indent_list_items = indent_list_items

    def _register(self, build_sys: BuildSystem, setup: LatexSetup) -> None:
        setup.emitter.register_block_or_inline(DisplayList, self._emit_list)
        setup.emitter.register_block_or_inline(DisplayListItem, self._emit_list_item)

    def _emit_list(
        self,
        list: DisplayList,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        mode = {
            DisplayListType.Itemize: "itemize",
            DisplayListType.Enumerate: "enumerate",
        }[list.list_type]
        with renderer.emit_env(mode):
            renderer.emit(*list.contents, joiner=renderer.emit_break_paragraph)

    def _emit_list_item(
        self,
        list_item: DisplayListItem,
        renderer: LatexRenderer,
        fmt: FmtEnv,
    ) -> None:
        # Put {} after \item so square brackets at the start of render_block don't get swallowed as arguments
        renderer.emit_raw("\\item{} ")
        indent_width = len("\\item{} ")
        with renderer.indent(indent_width):
            renderer.emit(list_item.contents)
