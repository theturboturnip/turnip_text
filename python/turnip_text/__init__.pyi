from typing import (
    Any,
    Dict,
    Iterator,
    List,
    Optional,
    Protocol,
    Sequence,
    TypeAlias,
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
    is_block: bool = True
    is_header: bool = True
    weight: int = 0

DocElement: TypeAlias = Union[Block, Inline]

@runtime_checkable
class BlocksBuilder(Protocol):
    def build_from_blocks(self, blocks: Blocks) -> Optional[DocElement]: ...

@runtime_checkable
class InlinesBuilder(Protocol):
    def build_from_inlines(self, inlines: Inlines) -> Optional[DocElement]: ...

@runtime_checkable
class RawScopeBuilder(Protocol):
    def build_from_raw(self, raw: Raw) -> Optional[DocElement]: ...

# The types that can be coerced into an Inline, in the order they are checked and attempted.
# Sequence[Inline] is coerced by wrapping it in an Inlines
CoercibleToInline = Union[Inline, str, Sequence[Inline], int, float]

# The types that can be coerced into an Inlines, in the order they are checked and attempted.
# 1. InlineScopes are passed through.
# 2. Coercion to Inline is attempted, and must succeed.
# 3. If it coerced to Inlines by the inline process (i.e. it was originally Sequence[Inline]),
# that Inlines is passed through.
# 4. Otherwise the plain Inline is wrapped in Inlines([plain_inline])
CoercibleToInlineScope = Union[Inlines, CoercibleToInline]

# The types that can be coerced into a Block, in the order they are checked and attempted
CoercibleToBlock = Union[Block, Sentence, Sequence[Block], CoercibleToInline]

# The types that can be coerced into a Blocks, in the order they are checked and attempted
CoercibleToBlocks = Union[Blocks, CoercibleToBlock]

def join_inlines(inlines: Sequence[Inline], joiner: Inline) -> Inlines:
    """Equivalent of string.join, but for joining any set of Inlines with a joiner Inline"""
    ...

def open_turnip_text_source(path: str, encoding: str = "utf-8") -> TurnipTextSource:
    """A shortcut for opening a file from a real filesystem as a TurnipTextSource"""
    ...

# Parsers return a Blocks of the top-level content, then a Document
def parse_file(
    file: TurnipTextSource,
    py_env: Dict[str, Any],
    recursion_warning: bool = True,
    max_file_depth: int = 128,
) -> Document: ...
def coerce_to_inline(obj: CoercibleToInline) -> Inline: ...
def coerce_to_inlines(obj: CoercibleToInlineScope) -> Inlines: ...
def coerce_to_block(obj: CoercibleToBlock) -> Block: ...
def coerce_to_blocks(obj: CoercibleToBlocks) -> Blocks: ...

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

class Blocks(Block):
    def __init__(self, seq: Optional[Sequence[Block]] = None): ...
    def __len__(self) -> int: ...
    # Iterate over the blocks in the Blocks
    def __iter__(self) -> Iterator[Block]: ...
    # Push a block into the Blocks
    def append_block(self, b: Block) -> None: ...
    # Insert a block before `index` in the Blocks
    def insert_block(self, index: int, b: Block) -> None: ...

class Inlines(Inline):
    def __init__(self, seq: Optional[Sequence[Inline]] = None): ...
    def __len__(self) -> int: ...
    # Iterate over the inline items in the Inlines
    def __iter__(self) -> Iterator[Inline]: ...
    # Push an inline item into the Inlines
    def append_inline(self, i: Inline) -> None: ...
    # Insert an inline before `index` in the Inlines
    def insert_inline(self, index: int, i: Inline) -> None: ...

class Document:
    def __init__(
        self,
        contents: Blocks,
        segments: Sequence[DocSegment],
    ): ...
    @property
    def contents(self) -> Blocks: ...
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
        contents: Blocks,
        subsegments: Sequence[DocSegment],
    ): ...
    @property
    def header(self) -> Header: ...
    @property
    def contents(self) -> Blocks: ...
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
