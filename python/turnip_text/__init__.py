from .turnip_text import UnescapedText, Sentence, Paragraph, BlockScope, block_scope_builder_generator, inline_scope_builder_generator, raw_scope_builder_generator, parse_file # type: ignore

import abc
from typing import Iterator, List, Protocol, runtime_checkable


class Inline(Protocol):
    pass #is_block: None

@runtime_checkable
class Block(Protocol):
    is_block: bool = True

@runtime_checkable
class BlockScopeBuilder(Protocol):
    def build_from_blocks(self, bs: BlockScope) -> Block: ...

@runtime_checkable
class InlineScopeBuilder(Protocol):
    def build_from_inlines(self, inls: List[Inline]) -> Inline: ...

@runtime_checkable
class RawScopeBuilder(Protocol):
    def build_from_raw(self, raw: str) -> Inline: ...
