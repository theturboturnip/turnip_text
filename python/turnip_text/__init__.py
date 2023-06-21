from typing import Optional, Protocol, Union, runtime_checkable

from ._native import (  # type: ignore
    BlockScope,
    InlineScope,
    Paragraph,
    RawText,
    Sentence,
    UnescapedText,
)
from ._native import parse_file as parse_file_native  # type: ignore
from ._native import parse_str as parse_str_native  # type: ignore


class Inline(Protocol):
    is_inline: bool = True


@runtime_checkable
class Block(Protocol):
    is_block: bool = True


@runtime_checkable
class BlockScopeBuilder(Protocol):
    def build_from_blocks(self, bs: BlockScope) -> Optional[Block]:
        ...


@runtime_checkable
class InlineScopeBuilder(Protocol):
    def build_from_inlines(self, inls: InlineScope) -> Inline:
        ...


@runtime_checkable
class RawScopeBuilder(Protocol):
    def build_from_raw(self, raw: str) -> Union[Inline, Block]:
        ...
