from dataclasses import dataclass
from enum import Enum
from typing import Iterable, List, Sequence, Union, cast

from typing_extensions import override

from turnip_text import Block, BlockScope, Inline
from turnip_text.doc.user_nodes import UserNode
from turnip_text.env_plugins import EnvPlugin
from turnip_text.helpers import block_scope_builder, block_scope_to_list


class DisplayListType(Enum):
    Enumerate = 0
    Itemize = 1


@dataclass(frozen=True)
class DisplayList(UserNode, Block):
    list_type: DisplayListType  # TODO could reuse Numbering from render.counters?
    contents: List[
        Union["DisplayList", "DisplayListItem"]
    ]  # TODO could replace DisplayListItem with Block? Auto-attach dots?
    anchor = None

    @override
    def child_nodes(self) -> Iterable[Block | Inline] | None:
        return self.contents


@dataclass(frozen=True)
class DisplayListItem(UserNode, Block):
    contents: BlockScope
    anchor = None

    @override
    def child_nodes(self) -> Iterable[Block | Inline] | None:
        return self.contents


class ListEnvPlugin(EnvPlugin):
    def _doc_nodes(self) -> Sequence[type[Block] | type[Inline]]:
        return (DisplayList, DisplayListItem)

    @block_scope_builder
    @staticmethod
    def num_list(contents: BlockScope) -> Block:
        items: List[DisplayListItem | DisplayList] = block_scope_to_list(
            contents,
            Union[DisplayList, DisplayListItem],
            error_context="[num_list]",
        )
        return DisplayList(
            list_type=DisplayListType.Enumerate,
            contents=cast(List[DisplayListItem | DisplayList], items),
        )

    @block_scope_builder
    @staticmethod
    def item_list(contents: BlockScope) -> Block:
        items: List[DisplayListItem | DisplayList] = block_scope_to_list(
            contents,
            Union[DisplayList, DisplayListItem],
            error_context="[item_list]",
        )
        return DisplayList(
            list_type=DisplayListType.Itemize,
            contents=items,
        )
    
    @block_scope_builder
    @staticmethod
    def itemize(contents: BlockScope) -> Block:
        """
        Deprecated legacy name,
        TODO remove
        """
        return ListEnvPlugin.item_list @ contents

    @block_scope_builder
    @staticmethod
    def item(block_scope: BlockScope) -> Block:
        return DisplayListItem(contents=block_scope)
