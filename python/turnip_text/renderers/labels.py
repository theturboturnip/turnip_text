"""
Labels and refs galore!

We want labels to refer to content, not just points in the text.
The content it refers to is one of a bunch of types:
- floats, like figures
- a section or other structural point
- text
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

from dataclasses import dataclass
from typing import List, Mapping

from turnip_text import UnescapedText

# e.g. arabic, roman numerals (lower and upper), alphabetic
Numbering = Mapping[int, UnescapedText]

@dataclass
class Label:
    id: str
    prefix: UnescapedText
    numbering: Numbering
    sublabels: List['Label']

class LabelMap:
    # The roots of the tree of labels
    label_tree: List[Label]
    # Mapping of Label.id to the Label in the tree of labels
    id_to_label: Mapping[str, Label]

    def __init__(self, label_tree: List[Label]) -> None:
        self.label_tree = label_tree
        self.id_to_label = {}
        
        raise NotImplementedError()