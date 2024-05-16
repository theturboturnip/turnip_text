from dataclasses import dataclass
from enum import Enum
from typing import Iterable, List, Sequence, Union, cast

from turnip_text import Block, Blocks, Inline
from turnip_text.doc.user_nodes import UserNode
from turnip_text.env_plugins import EnvPlugin
from turnip_text.helpers import block_scope_builder
from typing_extensions import override


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
    contents: Blocks
    anchor = None

    @override
    def child_nodes(self) -> Iterable[Block | Inline] | None:
        return self.contents


class ListEnvPlugin(EnvPlugin):
    def _doc_nodes(self) -> Sequence[type[Block] | type[Inline]]:
        return (DisplayList, DisplayListItem)

    @block_scope_builder
    @staticmethod
    def enumerate(blocks: Blocks) -> Block:
        items = list(blocks)
        if not all(isinstance(x, (DisplayListItem, DisplayList)) for x in items):
            raise TypeError(
                f"Found blocks in this list that were not list [item]s or other lists!"
            )
        return DisplayList(
            list_type=DisplayListType.Enumerate,
            contents=cast(List[DisplayListItem | DisplayList], items),
        )

    @block_scope_builder
    @staticmethod
    def itemize(blocks: Blocks) -> Block:
        items = list(blocks)
        if not all(isinstance(x, (DisplayListItem, DisplayList)) for x in items):
            raise TypeError(
                f"Found blocks in this list that were not list [item]s or other lists!"
            )
        return DisplayList(
            list_type=DisplayListType.Itemize,
            contents=cast(List[DisplayListItem | DisplayList], items),
        )

    @block_scope_builder
    @staticmethod
    def item(blocks: Blocks) -> Block:
        return DisplayListItem(contents=blocks)
