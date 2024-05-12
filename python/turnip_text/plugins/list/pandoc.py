import turnip_text.render.pandoc.pandoc_types as pan
from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.list import (
    DisplayList,
    DisplayListItem,
    DisplayListType,
    ListEnvPlugin,
)
from turnip_text.render.pandoc import PandocPlugin, PandocRenderer, PandocSetup


class PandocListPlugin(PandocPlugin, ListEnvPlugin):
    def _register(self, build_sys: BuildSystem, setup: PandocSetup) -> None:
        super()._register(build_sys, setup)
        setup.makers.register_block(DisplayList, self._build_list)
        # In case the DisplayListItem is ever put on its own, it's a no-op
        setup.makers.register_block(
            DisplayListItem,
            lambda item, renderer, fmt: renderer.make_block_scope(item.contents),
        )

    def _build_list(
        self, list: DisplayList, renderer: PandocRenderer, fmt: FmtEnv
    ) -> pan.Block:
        # pandoc demands List[List[Block]]
        list_contents = [
            (
                # If the item is a DisplayList, make a list-of-one with the sublist inside
                [self._build_list(item, renderer, fmt)]
                if isinstance(item, DisplayList)
                # Otherwise make a list directly of the contents of the block scope
                else renderer.make_block_scope_list(item.contents)
            )
            for item in list.contents
        ]
        if list.list_type == DisplayListType.Itemize:
            return pan.BulletList(list_contents)
        else:
            return pan.OrderedList(
                (1, pan.DefaultStyle(), pan.DefaultDelim()),
                list_contents,
            )
