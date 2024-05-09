import abc
from typing import (
    Iterable,
    Iterator,
    List,
    Optional,
    Protocol,
    Sequence,
    Union,
    runtime_checkable,
)

__all__ = [
    "Block",
    "BlockScope",
    "Inline",
    "InlineScope",
    "DocSegment",
    "Document",
    "Paragraph",
    "Raw",
    "Sentence",
    "Text",
    "coerce_to_block",
    "coerce_to_block_scope",
    "coerce_to_inline",
    "coerce_to_inline_scope",
    "BlockScopeBuilder",
    "InlineScopeBuilder",
    "RawScopeBuilder",
    "CoercibleToInline",
    "CoercibleToInlineScope",
    "CoercibleToBlock",
    "CoercibleToBlockScope",
    "parse_file",
    "TurnipTextError",
    "TurnipTextSource",
    "open_turnip_text_source",
]

from ._native import (  # type: ignore
    BlockScope,
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
    coerce_to_block_scope,
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
    is_header: bool = True
    weight: int = 0


@runtime_checkable
class BlockScopeBuilder(Protocol):
    def build_from_blocks(
        self, bs: BlockScope
    ) -> Union[Header, Block, Inline, None]: ...


@runtime_checkable
class InlineScopeBuilder(Protocol):
    def build_from_inlines(
        self, inls: InlineScope
    ) -> Union[Header, Block, Inline, None]: ...


@runtime_checkable
class RawScopeBuilder(Protocol):
    def build_from_raw(self, raw: str) -> Union[Header, Block, Inline, None]: ...


# The types that can be coerced into an Inline, in the order they are checked and attempted.
# List[Inline] is coerced by wrapping it in an InlineScope
CoercibleToInline = Union[Inline, List[Inline], str, int, float]

# The types that can be coerced into an InlineScope, in the order they are checked and attempted.
# 1. InlineScopes are passed through.
# 2. Coercion to Inline is attempted, and must succeed.
# 3. If it coerced to InlineScope by the inline process (i.e. it was originally List[Inline]),
# that InlineScope is passed through.
# 4. Otherwise the plain Inline is wrapped in InlineScope([plain_inline])
CoercibleToInlineScope = Union[InlineScope, CoercibleToInline]

# The types that can be coerced into a Block, in the order they are checked and attempted
CoercibleToBlock = Union[
    List[Block], Block, Paragraph, Sentence, CoercibleToInlineScope
]

# The types that can be coerced into a BlockScope, in the order they are checked and attempted
CoercibleToBlockScope = Union[BlockScope, CoercibleToBlock]


def join_inlines(inlines: Iterable[Inline], joiner: Inline) -> InlineScope:
    """Equivalent of string.join, but for joining any set of Inlines with a joiner Inline"""
    new_inlines = [val for i in inlines for val in (i, joiner)]
    if new_inlines:
        new_inlines.pop()
    return InlineScope(new_inlines)


def open_turnip_text_source(path: str, encoding: str = "utf-8") -> TurnipTextSource:
    """A shortcut for opening a file from a real filesystem as a TurnipTextSource"""
    with open(path, "r", encoding=encoding) as file:
        return TurnipTextSource.from_file(name=path, file=file)
