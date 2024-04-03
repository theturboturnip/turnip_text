import abc
from typing import (
    Any,
    Dict,
    Iterable,
    Iterator,
    List,
    Optional,
    Protocol,
    Union,
    runtime_checkable,
)

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
    def build_from_blocks(
        self, bs: BlockScope
    ) -> Optional[Block | DocSegmentHeader]: ...
    def __matmul__(
        self, maybe_b: "CoercibleToBlockScope"
    ) -> Optional[Block | DocSegmentHeader]:
        bs = coerce_to_block_scope(maybe_b)
        return self.build_from_blocks(bs)

class InlineScopeBuilder(abc.ABC):
    @abc.abstractmethod
    def build_from_inlines(self, inls: InlineScope) -> Inline: ...
    def __matmul__(self, maybe_inls: "CoercibleToInlineScope") -> Inline:
        inls = coerce_to_inline_scope(maybe_inls)
        return self.build_from_inlines(inls)

@runtime_checkable
class RawScopeBuilder(Protocol):
    def build_from_raw(self, raw: str) -> Union[Inline, Block]: ...

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
    ...

# Parsers return a BlockScope of the top-level content, then a DocSegment tree
def parse_file_native(file: TurnipTextSource, locals: Dict[str, Any]) -> DocSegment: ...
def coerce_to_inline(obj: CoercibleToInline) -> Inline: ...
def coerce_to_inline_scope(obj: CoercibleToInlineScope) -> InlineScope: ...
def coerce_to_block(obj: CoercibleToBlock) -> Block: ...
def coerce_to_block_scope(obj: CoercibleToBlockScope) -> BlockScope: ...

class Text(Inline):
    def __init__(self, text: str) -> None: ...
    @property
    def text(self) -> str: ...

class RawText(Inline):
    def __init__(self, text: str) -> None: ...
    @property
    def text(self) -> str: ...

# Note - Sentence is NOT an Inline. This means there's always a hierarchy of Paragraph -> many Sentences -> many Inlines.
# InlineScopes can be nested, Sentences cannot.
class Sentence:
    def __init__(self, list: Optional[List[Inline]] = None): ...
    def __len__(self) -> int: ...
    # Iterate over the inline blocks in the sentence
    def __iter__(self) -> Iterator[Inline]: ...
    # Push an inline node into the sentence
    def push_inline(self, node: Inline) -> None: ...

class Paragraph(Block):
    def __init__(self, list: Optional[List[Sentence]] = None): ...
    def __len__(self) -> int: ...
    # Iterate over the sentences in the Paragraph
    def __iter__(self) -> Iterator[Sentence]: ...
    # Push a sentence into the Paragraph
    def push_sentence(self, s: Sentence) -> None: ...

class BlockScope(Block):
    def __init__(self, list: Optional[List[Block]] = None): ...
    def __len__(self) -> int: ...
    # Iterate over the blocks in the BlockScope
    def __iter__(self) -> Iterator[Block]: ...
    # Push a block into the BlockScope
    def push_block(self, b: Block) -> None: ...

class InlineScope(Inline):
    def __init__(self, list: Optional[List[Inline]] = None): ...
    def __len__(self) -> int: ...
    # Iterate over the inline items in the InlineScope
    def __iter__(self) -> Iterator[Inline]: ...
    # Push an inline item into the InlineScope
    def push_inline(self, b: Inline) -> None: ...

class DocSegment:
    def __init__(
        self,
        header: DocSegmentHeader,
        contents: BlockScope,
        subsegments: List[DocSegment],
    ): ...
    @property
    def header(self) -> DocSegmentHeader: ...
    @property
    def contents(self) -> BlockScope: ...
    @property
    def subsegments(self) -> Iterator["DocSegment"]: ...
    # TODO make something that does correct weight checking??
    def push_subsegment(self, d: DocSegment) -> None: ...

class TurnipTextSource:
    """
    Emit an instance of this class from eval-brackets in a Block context to start parsing its contents instead.
    """

    def __init__(self, name: str, contents: str) -> None: ...
    @staticmethod
    def from_path(path: str) -> TurnipTextSource: ...
    @staticmethod
    def from_string(contents: str) -> TurnipTextSource: ...
