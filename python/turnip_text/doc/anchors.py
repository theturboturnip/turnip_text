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
from typing import (
    Dict,
    List,
    Mapping,
    MutableMapping,
    Optional,
    Sequence,
    Tuple,
    Type,
    Union,
)

from turnip_text import Block, Inline, InlineScope, InlineScopeBuilder


# Unlike the phd_notes lib version, this shouldn't be subclassed.
@dataclass(frozen=True)
class Anchor:
    kind: str
    id: Optional[str]  # If the id is None, you can't backreference this object. Ever.

    def canonical(self) -> str:
        return f"{self.kind}:{self.id}"

    def to_backref(self, label_contents: Optional[Inline] = None) -> "Backref":
        if self.id is None:
            raise ValueError(f"Can't convert an Anchor {self} with no id to a Backref")
        return Backref(self.id, self.kind, label_contents)

    def __str__(self) -> str:
        return self.canonical()


@dataclass(frozen=True)
class Backref(Inline, InlineScopeBuilder):
    id: str
    kind: Optional[
        str
    ]  # Usually there should be exactly one Anchor for every one ID. This is used to disambiguate otherwise
    label_contents: Optional[Inline]  # Override for label

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        assert self.label_contents is None
        return dataclasses.replace(self, label_contents=inls)


# Responsible for keeping track of all the anchors in a document
class DocAnchors:
    _anchor_id_to_possible_kinds: Dict[str, Dict[str, Anchor]]

    def __init__(self) -> None:
        self._anchor_id_to_possible_kinds = defaultdict(dict)

    def _doc_nodes(self) -> Sequence[type[Block] | type[Inline]]:
        return [Backref]

    def register_new_anchor(self, kind: str, id: Optional[str]) -> Anchor:
        """
        When inside the document, create a new anchor.
        """
        # TODO assert kind and id are strings if present?
        l = Anchor(
            kind=kind,
            id=id,
        )
        if id is not None:
            self._anchor_id_to_possible_kinds[id][kind] = l
        return l

    def lookup_backref(self, backref: Backref) -> Anchor:
        """
        Should be called by renderers to resolve a backref into an anchor.
        The renderer can then retrieve the counters for the anchor.
        """

        if backref.id not in self._anchor_id_to_possible_kinds:
            raise ValueError(
                f"Backref {backref} refers to an ID '{backref.id}' with no anchor!"
            )

        possible_kinds = self._anchor_id_to_possible_kinds[backref.id]

        if backref.kind is None:
            if len(possible_kinds) != 1:
                raise ValueError(
                    f"Backref {backref} doesn't specify the kind of anchor it's referring to, and there are multiple with that ID: {possible_kinds}"
                )
            only_possible_anchor = next(iter(possible_kinds.values()))
            return only_possible_anchor
        else:
            if backref.kind not in possible_kinds:
                raise ValueError(
                    f"Backref {backref} specifies an anchor of kind {backref.kind}, which doesn't exist for ID {backref.id}: {possible_kinds}"
                )
            return possible_kinds[backref.kind]
