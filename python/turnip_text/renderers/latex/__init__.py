from typing import Any, Dict, List, Optional, Tuple, Union
from dataclasses import dataclass
import uuid

from turnip_text import Block, BlockScope, BlockScopeBuilder, Inline, InlineScope, InlineScopeBuilder, UnescapedText, Paragraph, Sentence
from turnip_text.helpers import block_scope_builder, inline_scope_builder
from turnip_text.renderers import CiteKey, Renderer, dictify_pure_property

CiteKeyWithOptionNote = Tuple[CiteKey, Optional[str]]

@dataclass(frozen=True)
class FootnoteAnchor(Inline):
    label: str

@dataclass(frozen=True)
class HeadedBlock(Block):
    latex_name: str
    name: str
    contents: BlockScope
    label: Optional[str] = None
    num: bool = True

@dataclass(frozen=True)
class Citation(Inline):
    # List of (label, note?)
    labels: List[CiteKeyWithOptionNote]

@dataclass(frozen=True)
class Url(Inline):
    url: str

@dataclass(frozen=True)
class DisplayList(Block):
    # TODO allow nested lists
    #items: List[Union[BlockNode, List]]
    items: List
    mode: str
    is_block = True    

@dataclass
class Formatted(Inline):
    format_type: str # e.g. "emph"
    items: InlineScope

# TODO pytest for "does LatexRenderer support all functions of Renderer"
class LatexRenderer(Renderer):
    _citations: Dict[str, Any]
    _footnotes: Dict[str, Block]

    def __init__(self) -> None:
        self._citations = {}
        self._footnotes = {}

    # BEGIN: Renderer functions

    @dictify_pure_property
    def footnote(self) -> InlineScopeBuilder:
        @inline_scope_builder
        def footnote_builder(contents: InlineScope) -> Inline:
            label = str(uuid.uuid4())
            self._footnotes[label] = Paragraph([Sentence([contents])])
            return FootnoteAnchor(label)
        
        return footnote_builder
    
    def footnote_ref(self, label: str) -> Inline:
        return FootnoteAnchor(label)
        
    def footnote_text(self, label: str) -> BlockScopeBuilder:
        # Return a callable which is invoked with the contents of the following inline scope
        # Example usage:
        # [footnote_text("label")]{text}
        # equivalent to
        # [footnote_text("label")(r"text")]
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            self._footnotes[label] = contents
            return None
        return handle_block_contents

    def section(self, name: str, label: Optional[str]=None, num: bool=True) -> BlockScopeBuilder:
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            return HeadedBlock(
                latex_name="section",
                name=name,
                label=label,
                num=num,
                contents=contents
            )
        return handle_block_contents

    def subsection(self, name: str, label: Optional[str]=None, num: bool=True) -> BlockScopeBuilder:
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            return HeadedBlock(
                latex_name="subsection",
                name=name,
                label=label,
                num=num,
                contents=contents
            )
        return handle_block_contents

    def subsubsection(self, name: str, label: Optional[str]=None, num: bool=True) -> BlockScopeBuilder:
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            return HeadedBlock(
                latex_name="subsubsection",
                name=name,
                label=label,
                num=num,
                contents=contents
            )
        return handle_block_contents

    def cite(self, *labels: Union[str, Tuple[str, Optional[str]]]) -> Inline:
        # Convert ["label"] to [("label", None)] so Citation has a consistent format
        adapted_labels = [
            (label, None) if isinstance(label, str) else label
            for label in labels
        ]
        return Citation(adapted_labels)

    # TODO make this output \citeauthor
    def citeauthor(self, label: str) -> Inline:
        return Citation([(label, None)])

    def url(self, url: str) -> Inline:
        return Url(url)

    @dictify_pure_property
    def enumerate(self) -> BlockScopeBuilder:
        @block_scope_builder
        def enumerate_builder(contents: BlockScope) -> Block:
            return DisplayList(mode="enumerate", items=list(contents))
        return enumerate_builder

    @dictify_pure_property
    def item(self) -> BlockScopeBuilder:
        @block_scope_builder
        def item_builder(block_scope: BlockScope) -> Block:
            # TODO some sort of Item() wrapper class?
            return block_scope
        return item_builder

    @dictify_pure_property
    def emph(self) -> InlineScopeBuilder:
        @inline_scope_builder
        def emph_builder(items: InlineScope) -> Inline:
            return Formatted("emph", items)
        return emph_builder

    # TODO this should be RawLatexText instead of UnescapedText
    OPEN_DQUOTE = UnescapedText("``")
    CLOS_DQUOTE = UnescapedText("''")
    @dictify_pure_property
    def enquote(self) -> InlineScopeBuilder:
        @inline_scope_builder
        def enquote_builder(items: InlineScope) -> Inline:
            return InlineScope([LatexRenderer.OPEN_DQUOTE] + list(items) + [LatexRenderer.CLOS_DQUOTE])
        return enquote_builder
