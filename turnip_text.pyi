from typing import (
    Any,
    Dict,
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

def parse_file_native(path: str, locals: Dict[str, Any]) -> BlockScope: ...
def parse_str_native(data: str, locals: Dict[str, Any]) -> BlockScope: ...
def coerce_to_inline(obj: CoercibleToInline) -> Inline: ...
def coerce_to_inline_scope(obj: CoercibleToInlineScope) -> InlineScope: ...
def coerce_to_block(obj: CoercibleToBlock) -> Block: ...
def coerce_to_block_scope(obj: CoercibleToBlockScope) -> BlockScope: ...

class UnescapedText(Inline):
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
