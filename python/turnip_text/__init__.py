from os import PathLike
from .turnip_text import UnescapedText, Sentence, Paragraph, BlockScope, InlineScope # type: ignore
from .turnip_text import parse_file as parse_file_local # type: ignore
from .renderers import Renderer

def parse_file(p: PathLike, r: Renderer) -> BlockScope:
    # TODO this seems super icky
    # The problem: we want to be able to call e.g. [footnote] inside the turniptext file.
    # But if footnote were a free function, it would mutate global state -- we don't want that.
    # A hack! Require a 'renderer object' to be passed in - this encapsulates the local state.
    # Create a new dictionary, holding all of the Renderer's public 'bound methods',
    # and use that as the locals for parse_file_local.
    public_r_fields = dict()
    for name in dir(r):
        if not name.startswith("__"):
            public_r_fields[name] = getattr(r, name)
    return parse_file_local(str(p), public_r_fields)

from typing import List, Protocol, Tuple, runtime_checkable


class Inline(Protocol):
    is_inline: bool = True

@runtime_checkable
class Block(Protocol):
    is_block: bool = True

@runtime_checkable
class BlockScopeBuilder(Protocol):
    def build_from_blocks(self, bs: BlockScope) -> Block: ...

@runtime_checkable
class InlineScopeBuilder(Protocol):
    def build_from_inlines(self, inls: InlineScope) -> Inline: ...

@runtime_checkable
class RawScopeBuilder(Protocol):
    def build_from_raw(self, raw: str) -> Inline: ...
