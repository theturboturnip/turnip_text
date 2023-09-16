"""
Labels and refs galore!

We want labels to refer to content, not just points in the text.
The content it refers to is one of a bunch of types:
- floats, like figures
- a section or other structural point
- text
- footnotes?
- citation marks?
- any custom types that may or may not be floats: code listings, tables, equations, e.g. my hypotheses for the mphil diss

References to specific text are a special case: the user needs to choose how to refer back to them, maybe by closest-structural-counter, maybe they're in some float (is there a floating quote environment?), maybe by page...
It gets weird there too because some formats don't have pages. Markdown would probably change "on page X" to a link "here"?.

These "label kinds" determine the in-text references to each item, and might (per-renderer) determine the kind of \label used.
We need to find a way to make it work kinda like LaTeX - LaTeX lets you associated labels with multiple counters, and choose when counters reset, so you can say "figure 1.2.4" is figure 4 in chapter 1 in section 2.
This system also needs to support creating new sub-labels: e.g. if I already have a "figure" label, I want to create a sub-figure to represent "figure 1.2.4.a"

Clearly we need to make a hierarchy of some kind. 
Represent it as an acyclic tree.
If a LaTeX renderer wants, it could choose to implement that through configuring counters in the preamble

Dad recommends a separate Counting phase once the document has been constructed and before rendered.
This requires me to make Blocks and Inlines implement "here are my children" so I have a consistent way to DFS it and then run counting.
"""

import abc
import dataclasses
from collections import defaultdict
from dataclasses import dataclass
from typing import Dict, List, Mapping, MutableMapping, Optional, Sequence, Tuple

from turnip_text import (
    Inline,
    InlineScope,
    InlineScopeBuilder,
    UnescapedText,
    coerce_to_inline_scope,
)
from turnip_text.renderers import StatelessContext

# e.g. arabic, roman numerals (lower and upper), alphabetic
Numbering = Mapping[int, UnescapedText]

@dataclass
class LabelKind(abc.ABC):
    id: str
    sublabels: List['LabelKind']
    numbering: Numbering

    @abc.abstractmethod
    def default_backref_text(self, ctx: StatelessContext, numbers: Sequence[Tuple[Numbering, int]]) -> Inline:
        ...

@dataclass
class BasicLabelKind(LabelKind):
    prefix: str

    def default_backref_text(self, ctx: StatelessContext, numbers: Sequence[Tuple[Numbering, int]]) -> Inline:
        return coerce_to_inline_scope(
            f"{self.prefix} {'.'.join(numbering[value] for numbering, value in numbers)}"
        )

class LabelKindMap:
    # The roots of the tree of labels
    label_kind_tree: List[LabelKind]
    # Mapping of Label.id to the Label in the tree of labels
    label_kind_lookup: Mapping[str, LabelKind]

    def __init__(self, label_tree: List[LabelKind]) -> None:
        self.label_kind_tree = label_tree
        self.label_kind_lookup = {}
        
        raise NotImplementedError()
    

# Unlike the phd_notes lib version, this shouldn't be subclassed.
@dataclass(frozen=True)
class Label:
    kind: str
    id: Optional[str] # If the id is None, you can't backreference this object. Ever.
    number: Tuple[int, ...] # The numbers for all label kinds that parent this kind, and the number for this label. e.g. if figures are numbered per chapter per section, this field is (chapter no., section no., figure no.). All zero-indexed.

    def canonical(self) -> str:
        return f"{self.kind}:{self.id}"
    
    def __str__(self) -> str:
        return self.canonical()

@dataclass(frozen=True)
class Backref(Inline, InlineScopeBuilder):
    id: str
    kind: Optional[str] # Usually there should be exactly one Label for every one ID. This is used to disambiguate otherwise
    label_contents: Optional[Inline] # Override for label

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        assert self.label_contents is None
        return dataclasses.replace(self, label_contents=inls)

# Responsible for keeping track of all the labels in a document
class DocumentLabelRefState:
    _label_kind_map: LabelKindMap
    _label_kind_count: Dict[str, int]
    _label_id_to_possible_kinds: Dict[str, Dict[str, Label]]

    def __init__(self, label_kind_map: LabelKindMap) -> None:
        self._label_kind_map = label_kind_map
        self._label_kind_count = defaultdict(lambda: 0)
        self._label_id_to_possible_kinds = defaultdict(dict)

    # TODO make this need to be called with unfrozen state
    def register_new_label(self, kind: str, id: Optional[str]) -> Label:
        # TODO this numbering shouldn't be done here, it should be done in a separate pass.
        # For now it's fine, but if we later implement e.g. float reordering for different output formats
        # then it would be wrong.
        number = self._label_kind_count[kind]
        self._label_kind_count[kind] = number + 1
        l = Label(
            kind=kind,
            id=id,
            # TODO how the hell do we track the upper numbers. We need a counter class or something
            number=number
        )
        if id is not None:
            self._label_id_to_possible_kinds[id][kind] = l
        return l
    
    # TODO make this need to be called with frozen state
    def lookup_backref(self, backref: Backref) -> Label:
        if backref.id not in self._label_id_to_possible_kinds:
            raise ValueError(f"Backref {backref} refers to an ID '{backref.id}' with no label!")
        
        possible_kinds = self._label_id_to_possible_kinds[backref.id]

        if backref.kind is None:
            if len(possible_kinds) != 1:
                raise ValueError(f"Backref {backref} doesn't specify the kind of label it's referring to, and there are multiple with that ID: {possible_kinds}")
            only_possible_label = next(iter(possible_kinds.values()))
            return only_possible_label
        else:
            if backref.kind not in possible_kinds:
                raise ValueError(f"Backref {backref} specifies an label of kind {backref.kind}, which doesn't exist for ID {backref.id}: {possible_kinds}")
            return possible_kinds[backref.kind]
        
    # TODO make this need to be called with frozen state
    def resolve_backref_text(self, backref: Backref) -> Inline:
        label = self.lookup_backref(backref)
