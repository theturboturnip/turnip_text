from typing import Optional, Protocol, Sequence, TypeAlias, Union, runtime_checkable

__all__ = [
    "Block",
    "Blocks",
    "Inline",
    "InlineScope",
    "DocSegment",
    "Document",
    "Paragraph",
    "Raw",
    "Sentence",
    "Text",
    "coerce_to_block",
    "coerce_to_blocks",
    "coerce_to_inline",
    "coerce_to_inline_scope",
    "BlocksBuilder",
    "InlineScopeBuilder",
    "RawScopeBuilder",
    "CoercibleToInline",
    "CoercibleToInlineScope",
    "CoercibleToBlock",
    "CoercibleToBlocks",
    "parse_file",
    "TurnipTextError",
    "TurnipTextSource",
    "open_turnip_text_source",
]

from ._native import (  # type: ignore
    Blocks,
    DocSegment,
    Document,
    InlineScope,
    Paragraph,
    Raw,
    Sentence,
    Text,
    TurnipTextError,
    TurnipTextSource,
    coerce_to_block,
    coerce_to_blocks,
    coerce_to_inline,
    coerce_to_inline_scope,
    parse_file,
)

# Block, Inline, Header, and the Builders are all typeclasses that we can't import directly.


@runtime_checkable
class Inline(Protocol):
    is_inline: bool = True


@runtime_checkable
class Block(Protocol):
    is_block: bool = True


@runtime_checkable
class Header(Protocol):
    is_block: bool = True
    is_header: bool = True
    weight: int = 0


DocElement: TypeAlias = Union[Block, Inline]


@runtime_checkable
class BlocksBuilder(Protocol):
    def build_from_blocks(self, blocks: Blocks) -> Optional[DocElement]: ...


@runtime_checkable
class InlineScopeBuilder(Protocol):
    def build_from_inlines(self, inlines: InlineScope) -> Optional[DocElement]: ...


@runtime_checkable
class RawScopeBuilder(Protocol):
    def build_from_raw(self, raw: Raw) -> Optional[DocElement]: ...


# The types that can be coerced into an Inline, in the order they are checked and attempted.
# Sequence[Inline] is coerced by wrapping it in a list and wrapping that in an InlineScope
CoercibleToInline = Union[Inline, str, Sequence[Inline], int, float]

# The types that can be coerced into an InlineScope, in the order they are checked and attempted.
# 1. InlineScopes are passed through.
# 2. Coercion to Inline is attempted, and must succeed.
# 3. If it coerced to InlineScope by the inline process (i.e. it was originally Sequence[Inline]),
# that InlineScope is passed through.
# 4. Otherwise the plain Inline is wrapped in InlineScope([plain_inline])
CoercibleToInlineScope = Union[InlineScope, CoercibleToInline]

# The types that can be coerced into a Block, in the order they are checked and attempted
CoercibleToBlock = Union[Block, Sentence, Sequence[Block], CoercibleToInline]

# The types that can be coerced into a Blocks, in the order they are checked and attempted
CoercibleToBlocks = Union[Blocks, CoercibleToBlock]


def join_inlines(inlines: Sequence[Inline], joiner: Inline) -> InlineScope:
    """Equivalent of string.join, but for joining any set of Inlines with a joiner Inline"""
    new_inlines = [val for i in inlines for val in (i, joiner)]
    if new_inlines:
        new_inlines.pop()
    return InlineScope(new_inlines)


def open_turnip_text_source(path: str, encoding: str = "utf-8") -> TurnipTextSource:
    """A shortcut for opening a file from a real filesystem as a TurnipTextSource"""
    with open(path, "r", encoding=encoding) as file:
        return TurnipTextSource.from_file(name=path, file=file)
