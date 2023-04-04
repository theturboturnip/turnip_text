from typing import Protocol, runtime_checkable

from .turnip_text import (  # type: ignore
    BlockScope,
    InlineScope,
    Paragraph,
    Sentence,
    UnescapedText,
)
from .turnip_text import parse_file as parse_file_native  # type: ignore


class Inline(Protocol):
    is_inline: bool = True


@runtime_checkable
class Block(Protocol):
    is_block: bool = True


@runtime_checkable
class BlockScopeBuilder(Protocol):
    def build_from_blocks(self, bs: BlockScope) -> Block:
        ...


@runtime_checkable
class InlineScopeBuilder(Protocol):
    def build_from_inlines(self, inls: InlineScope) -> Inline:
        ...


@runtime_checkable
class RawScopeBuilder(Protocol):
    def build_from_raw(self, raw: str) -> Inline:
        ...
