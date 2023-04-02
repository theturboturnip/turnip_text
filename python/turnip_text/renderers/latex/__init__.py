from typing import Any, Dict, List, Optional, Tuple, Union
from dataclasses import dataclass
import uuid

from turnip_text import Block, BlockScope, BlockScopeBuilder, Inline, InlineScope, InlineScopeBuilder, UnescapedText, Paragraph, Sentence
from turnip_text.helpers import block_scope_builder, inline_scope_builder

def latex_escape(text: UnescapedText):
    # TODO
    return str(text)

FOOTNOTE_TEXT: Dict[str, Block] = {}
@dataclass(frozen=True)
class FootnoteAnchor(Inline):
    label: str

    def render(self):
        # TODO maybe have the footnote_text define the text there?
        return r"\footnote{" + latex_escape(FOOTNOTE_TEXT[self.label]) + "}"

def footnote(label: Optional[str]=None) -> InlineScopeBuilder:
    if label is None:
        label = str(uuid.uuid4())

    l: str = label

    @inline_scope_builder
    def handle_inline_contents(contents: InlineScope) -> Inline:
        FOOTNOTE_TEXT[l] = Paragraph([Sentence([contents])])
        return FootnoteAnchor(l)
    return handle_inline_contents

def footnote_text(label: str):
    # Return a callable which is invoked with the contents of the following inline scope
    # Example usage:
    # [footnote_text("label")]{text}
    # equivalent to
    # [footnote_text("label")(r"text")]
    @block_scope_builder
    def handle_block_contents(contents: BlockScope) -> Block:
        FOOTNOTE_TEXT[label] = contents
        return None
    return handle_block_contents

@dataclass(frozen=True)
class HeadedBlock:
    latex_name: str
    name: str
    contents: BlockScope
    label: Optional[str] = None
    num: bool = True

    is_block = True

    def render(self):
        s_head = "\\" + self.latex_name
        if not self.num:
            s_head += "*"
        s_head += "{" + latex_escape(self.name) + "}"
        if self.label:
            s_head += r"\label{" + self.label + "}"
        return s_head

def section(name: str, label: Optional[str]=None, num: bool=True) -> BlockScopeBuilder:
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

def subsection(name: str, label: Optional[str]=None, num: bool=True) -> BlockScopeBuilder:
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

def subsubsection(name: str, label: Optional[str]=None, num: bool=True) -> BlockScopeBuilder:
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

CITATIONS: Dict[str, Any] = {}

@dataclass(frozen=True)
class Citation(Inline):
    # List of (label, note)
    labels: List[Tuple[str, Optional[str]]]

    def render(self):
        return "".join([
            r"\cite" + f"[{latex_escape(note)}]" if note else "" + "{" + label + "}"
            for label, note in self.labels
        ])

def cite(*labels: Union[str, Tuple[str, Optional[str]]]) -> Inline:
    # Convert ["label"] to [("label", None)] so Citation has a consistent format
    adapted_labels = [
        (label, None) if isinstance(label, str) else label
        for label in labels
    ]
    return Citation(adapted_labels)

# TODO make this output \citeauthor
def citeauthor(label: str) -> Inline:
    return Citation([(label, None)])

@dataclass(frozen=True)
class Url(Inline):
    url: str

def url(url: str) -> Inline:
    return Url(url)

@dataclass(frozen=True)
class DisplayList(Block):
    # TODO allow nested lists
    #items: List[Union[BlockNode, List]]
    items: List
    mode: str
    is_block = True    

    def render(self):
        # TODO nesting!
        # TODO \begin{self.mode}
        # TODO for item in items:
            # TODO latex_render_blocknode(item)
        # TODO \end{self.mode}
        raise NotImplementedError()


def enumerate():
    @block_scope_builder
    def handle_block_contents(contents: List[Block]) -> Block:
        return DisplayList(mode="enumerate", items=contents)
    return handle_block_contents

def item():
    @block_scope_builder
    def inner(block_scope: BlockScope) -> Block:
        # TODO some sort of Item() wrapper class?
        return block_scope
    return inner

@dataclass
class Formatted(Inline):
    format_type: str # e.g. "emph"
    items: InlineScope

    def render(self):
        # TODO return "\" + self.format_type + "{" + render(self.items) + "}"
        raise NotImplementedError()

# Because we want to use this like [emph]
# mark it as "build_from_inlines" manually
@inline_scope_builder
def emph(items: InlineScope) -> Inline:
    return Formatted("emph", items)

# TODO this should be RawLatexText instead of UnescapedText
OPEN_DQUOTE = UnescapedText("``")
CLOS_DQUOTE = UnescapedText("''")
@inline_scope_builder
def enquote(items: InlineScope) -> Inline:
    return InlineScope([OPEN_DQUOTE] + list(items) + [CLOS_DQUOTE])
