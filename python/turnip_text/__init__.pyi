from typing import (
    Any,
    Dict,
    Iterator,
    List,
    Optional,
    Protocol,
    Sequence,
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
# Sequence[Inline] is coerced by wrapping it in an InlineScope
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

# The types that can be coerced into a BlockScope, in the order they are checked and attempted
CoercibleToBlockScope = Union[BlockScope, CoercibleToBlock]

def join_inlines(inlines: Sequence[Inline], joiner: Inline) -> InlineScope:
    """Equivalent of string.join, but for joining any set of Inlines with a joiner Inline"""
    ...

def open_turnip_text_source(path: str, encoding: str = "utf-8") -> TurnipTextSource:
    """A shortcut for opening a file from a real filesystem as a TurnipTextSource"""
    ...

# Parsers return a BlockScope of the top-level content, then a Document
def parse_file(
    file: TurnipTextSource,
    py_env: Dict[str, Any],
    recursion_warning: bool = True,
    max_file_depth: int = 128,
) -> Document: ...
def coerce_to_inline(obj: CoercibleToInline) -> Inline: ...
def coerce_to_inline_scope(obj: CoercibleToInlineScope) -> InlineScope: ...
def coerce_to_block(obj: CoercibleToBlock) -> Block: ...
def coerce_to_block_scope(obj: CoercibleToBlockScope) -> BlockScope: ...

class Text(Inline):
    def __init__(self, text: str) -> None: ...
    @property
    def text(self) -> str: ...

class Raw(Inline):
    def __init__(self, text: str) -> None: ...
    @property
    def data(self) -> str: ...

# Note - Sentence is NOT an Inline. This means there's always a hierarchy of Paragraph -> many Sentences -> many Inlines.
# InlineScopes can be nested, Sentences cannot.
class Sentence:
    def __init__(self, seq: Optional[Sequence[Inline]] = None): ...
    def __len__(self) -> int: ...
    # Iterate over the inline blocks in the sentence
    def __iter__(self) -> Iterator[Inline]: ...
    # Push an inline node into the sentence
    def append_inline(self, i: Inline) -> None: ...
    # Insert an inline before `index` in the Sentence
    def insert_inline(self, index: int, i: Inline) -> None: ...

class Paragraph(Block):
    def __init__(self, seq: Optional[Sequence[Sentence]] = None): ...
    def __len__(self) -> int: ...
    # Iterate over the sentences in the Paragraph
    def __iter__(self) -> Iterator[Sentence]: ...
    # Push a sentence into the Paragraph
    def append_sentence(self, s: Sentence) -> None: ...
    # Insert a sentence before `index` in the Paragraph
    def insert_sentence(self, index: int, s: Sentence) -> None: ...

class BlockScope(Block):
    def __init__(self, seq: Optional[Sequence[Block]] = None): ...
    def __len__(self) -> int: ...
    # Iterate over the blocks in the BlockScope
    def __iter__(self) -> Iterator[Block]: ...
    # Push a block into the BlockScope
    def append_block(self, b: Block) -> None: ...
    # Insert a block before `index` in the BlockScope
    def insert_block(self, index: int, b: Block) -> None: ...

class InlineScope(Inline):
    def __init__(self, seq: Optional[Sequence[Inline]] = None): ...
    def __len__(self) -> int: ...
    # Iterate over the inline items in the InlineScope
    def __iter__(self) -> Iterator[Inline]: ...
    # Push an inline item into the InlineScope
    def append_inline(self, i: Inline) -> None: ...
    # Insert an inline before `index` in the InlineScope
    def insert_inline(self, index: int, i: Inline) -> None: ...

class Document:
    def __init__(
        self,
        contents: BlockScope,
        segments: Sequence[DocSegment],
    ): ...
    @property
    def contents(self) -> BlockScope: ...
    @property
    def segments(self) -> Iterator["DocSegment"]: ...
    # In order to create new DocSegments correctly, use append_header() and insert_header()
    # These call into the DocSegmentList to make sure you don't create invalid trees.
    # For example, if you have a Document with segments `[section, section]`,
    # `append_header(subsection)` will push the subsection into the last `section` instead of appending it at the same level as the other `sections`.
    def append_header(self, h: Header) -> DocSegment: ...
    def insert_header(self, index: int, h: Header) -> DocSegment: ...

class DocSegment:
    def __init__(
        self,
        header: Header,
        contents: BlockScope,
        subsegments: Sequence[DocSegment],
    ): ...
    @property
    def header(self) -> Header: ...
    @property
    def contents(self) -> BlockScope: ...
    @property
    def subsegments(self) -> Iterator["DocSegment"]: ...
    # In order to create new DocSegments correctly, use append_header() and insert_header()
    # These call into the DocSegmentList to make sure you don't create invalid trees.
    # For example, if you have a DocSegment with subsegments `[section, section]`,
    # `append_header(subsection)` will push the subsection into the last `section` instead of appending it at the same level as the other `sections`.
    def append_header(self, h: Header) -> DocSegment: ...
    def insert_header(self, index: int, h: Header) -> DocSegment: ...

class TextReadable(Protocol):
    """The protocol expected by TurnipTextSource.from_file().

    Any file obtained through open(path, "r") will be suitable.
    Files opened in bytes mode e.g. open(path, "rb") are not suitable, because they read out bytes instead of a str.
    """

    def read(self) -> str: ...

class TurnipTextSource:
    """
    Emit an instance of this class from eval-brackets while in block-mode to start parsing its contents instead.
    """

    def __init__(self, name: str, contents: str) -> None: ...
    @staticmethod
    def from_file(name: str, file: TextReadable) -> TurnipTextSource: ...
    @staticmethod
    def from_string(contents: str) -> TurnipTextSource: ...

class TurnipTextError(Exception): ...
