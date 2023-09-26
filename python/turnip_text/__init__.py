import abc
from typing import (
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
    "Paragraph",
    "RawText",
    "Sentence",
    "UnescapedText",
    "coerce_to_block_scope",
    "coerce_to_inline_scope",
    "BlockScopeBuilder",
    "InlineScopeBuilder",
    "RawScopeBuilder",
    "CoercibleToInline",
    "CoercibleToInlineScope",
    "CoercibleToBlock",
    "CoercibleToBlockScope",
    "parse_file_native",
    "parse_str_native",
]

from ._native import (  # type: ignore
    BlockScope,
    DocSegment,
    InlineScope,
    Paragraph,
    RawText,
    Sentence,
    UnescapedText,
    coerce_to_block_scope,
    coerce_to_inline_scope,
)
from ._native import parse_file as parse_file_native
from ._native import parse_str as parse_str_native


@runtime_checkable
class Inline(Protocol):
    is_inline: bool = True


@runtime_checkable
class Block(Protocol):
    is_block: bool = True


@runtime_checkable
class DocSegmentHeader(Protocol):
    is_segment_header: bool = True
    weight: int = 0


class BlockScopeBuilder(abc.ABC):
    @abc.abstractmethod
    def build_from_blocks(self, bs: BlockScope) -> Optional[Block | DocSegment]:
        ...

    def __matmul__(
        self, maybe_b: "CoercibleToBlockScope"
    ) -> Optional[Block | DocSegment]:
        bs = coerce_to_block_scope(maybe_b)
        return self.build_from_blocks(bs)


class InlineScopeBuilder(abc.ABC):
    @abc.abstractmethod
    def build_from_inlines(self, inls: InlineScope) -> Inline | DocSegment:
        ...

    def __matmul__(self, maybe_inls: "CoercibleToInlineScope") -> Inline | DocSegment:
        inls = coerce_to_inline_scope(maybe_inls)
        return self.build_from_inlines(inls)


@runtime_checkable
class RawScopeBuilder(Protocol):
    def build_from_raw(self, raw: str) -> Union[Inline, Block]:
        ...


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
