from typing import Generator

from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.list import (
    DisplayList,
    DisplayListItem,
    DisplayListType,
    ListEnvPlugin,
)
from turnip_text.render.markdown.renderer import (
    MarkdownPlugin,
    MarkdownRenderer,
    MarkdownSetup,
)


class MarkdownListPlugin(MarkdownPlugin, ListEnvPlugin):
    indent_list_items: bool = True

    def __init__(self, indent_list_items: bool = True):
        self.indent_list_items = indent_list_items

    def _register(self, build_sys: BuildSystem, setup: MarkdownSetup) -> None:
        setup.emitter.register_block_or_inline(DisplayList, self._emit_list)
        setup.emitter.register_block_or_inline(DisplayListItem, self._emit_list_item)

    def _emit_list_item(
        self,
        list_item: DisplayListItem,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        pass  # DisplayListItems inside DisplayLists will be handled directly

    def _emit_list(
        self,
        list: DisplayList,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        numbered = list.list_type == DisplayListType.Enumerate
        if renderer.in_html_mode:

            def emit_elem() -> Generator[None, None, None]:
                for item in list.contents:
                    renderer.emit_raw("<li>")
                    renderer.emit_newline()
                    with renderer.indent(4):
                        if isinstance(item, DisplayList):
                            renderer.emit_block(item)
                        else:
                            renderer.emit_blockscope(item.contents)
                    renderer.emit_newline()
                    renderer.emit_raw("</li>")
                    yield None

            tag = "ol" if numbered else "ul"
            renderer.emit_raw(f"<{tag}>\n")
            with renderer.indent(4):
                renderer.emit_join_gen(emit_elem(), renderer.emit_break_sentence)
            renderer.emit_raw(f"</{tag}>")
        else:
            if numbered:

                def emit_numbered() -> Generator[None, None, None]:
                    for idx, item in enumerate(list.contents):
                        indent = f"{idx+1}. "
                        renderer.emit_raw(indent)
                        with renderer.indent(len(indent)):
                            if isinstance(item, DisplayList):
                                renderer.emit_block(item)
                            else:
                                renderer.emit_blockscope(item.contents)
                        yield None

                renderer.emit_join_gen(emit_numbered(), renderer.emit_break_sentence)
            else:

                def emit_dashed() -> Generator[None, None, None]:
                    for idx, item in enumerate(list.contents):
                        indent = f"- "
                        renderer.emit_raw(indent)
                        with renderer.indent(len(indent)):
                            if isinstance(item, DisplayList):
                                renderer.emit_block(item)
                            else:
                                renderer.emit_blockscope(item.contents)
                        yield None

                renderer.emit_join_gen(emit_dashed(), renderer.emit_break_sentence)
