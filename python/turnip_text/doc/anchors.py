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

import dataclasses
from typing import Optional

from turnip_text import Inline, InlineScope, InlineScopeBuilder


@dataclasses.dataclass(frozen=True)
class Anchor(Inline):
    """An Anchor in the file which can always be referenced back to using a Backref.

    Includes a kind, e.g. 'footnote', and an ID which the user can use to interrupt"""

    kind: str
    id: str

    def canonical(self) -> str:
        return f"{self.kind}:{self.id}"

    def to_backref(self, label_contents: Optional[Inline] = None) -> "Backref":
        return Backref(self.id, self.kind, label_contents)

    def __str__(self) -> str:
        return self.canonical()


@dataclasses.dataclass(frozen=True)
class Backref(Inline, InlineScopeBuilder):
    """A reference to an Anchor in the file, which can optionally have a custom label.

    Must include the ID of the Anchor, but does not need to include the kind.
    We assume there is usually one Anchor for every ID, so you'd only need the kind to disambiguate between
    e.g. a figure with ID='fred' and a footnote with ID='fred'."""

    id: str
    kind: Optional[
        str
    ]  # Usually there should be exactly one Anchor for every one ID. This is used to disambiguate otherwise
    label_contents: Optional[Inline]  # Override for label

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        assert self.label_contents is None
        return dataclasses.replace(self, label_contents=inls)
