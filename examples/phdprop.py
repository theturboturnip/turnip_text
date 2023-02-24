from typing import List, Optional, Tuple, Union
from turnip_text import *
from dataclasses import dataclass

# TODO - move this logic into a Python-implemented LaTeX renderer library

def latex_escape(text: UnescapedText):
    # TODO
    return str(text)

FOOTNOTE_TEXT = {}
@dataclass(frozen=True)
class FootnoteAnchor:
    label: str

    def render(self):
        # TODO maybe have the footnote_text define the text there?
        return r"\footnote{" + latex_escape(FOOTNOTE_TEXT[self.label]) + "}"

def footnote(label: str) -> FootnoteAnchor:
    return FootnoteAnchor(label)

@inline_scope_owner
def footnote_text(label: str):
    # Return a callable which is invoked with the contents of the following inline scope
    # Example usage:
    # [footnote_text("label")]{text}
    # equivalent to
    # [footnote_text("label")(r"text")]
    def handle_inline_contents(text: UnescapedText):
        FOOTNOTE_TEXT[label] = text
    return handle_inline_contents

@dataclass(frozen=True)
class Header:
    latex_name: str
    name: str
    contents: List
    label: Optional[str] = None
    num: bool = True

    def render(self):
        s_head = "\\" + self.latex_name
        if not self.num:
            s_head += "*"
        s_head += "{" + latex_escape(self.name) + "}"
        if self.label:
            s_head += r"\label{" + self.label + "}"
        return s_head

@block_scope_owner
def section(name: str, label: Optional[str]=None, num: bool=True) -> Header:
    def handle_block_contents(contents: List):
        return Header(
            latex_name="section",
            name=name,
            label=label,
            num=num,
            contents=contents
        )
    return handle_block_contents

@block_scope_owner
def subsection(name: str, label: Optional[str]=None, num: bool=True) -> Header:
    def handle_block_contents(contents: List):
        return Header(
            latex_name="subsection",
            name=name,
            label=label,
            num=num,
            contents=contents
        )
    return handle_block_contents

@block_scope_owner
def subsubsection(name: str, label: Optional[str]=None, num: bool=True) -> Header:
    def handle_block_contents(contents: List):
        return Header(
            latex_name="subsubsection",
            name=name,
            label=label,
            num=num,
            contents=contents
        )
    return handle_block_contents

CITATIONS = {}

@dataclass(frozen=True)
class Citation:
    # List of (label, note)
    labels: List[Tuple[str, Optional[str]]]

    def render(self):
        return "".join([
            r"\cite" + f"[{latex_escape(note)}]" if note else "" + "{" + label + "}"
            for label, note in self.labels
        ])

def cite(*labels: List[Union[str, Tuple[str, Optional[str]]]]):
    # Convert ["label"] to [("label", None)] so Citation has a consistent format
    adapted_labels = [
        (label, None) if isinstance(label, str) else label
        for label in labels
    ]
    return Citation(adapted_labels)

# TODO make this output \citeauthor
def citeauthor(label: str):
    return Citation([(label, None)])

@dataclass(frozen=True)
class Url:
    url: str

def url(url: str):
    return Url(url)

@dataclass(frozen=True)
class DisplayList:
    # TODO allow nested lists
    #items: List[Union[BlockNode, List]]
    items: List
    mode: str
    
    def render(self):
        # TODO nesting!
        # TODO \begin{self.mode}
        # TODO for item in items:
            # TODO latex_render_blocknode(item)
        # TODO \end{self.mode}
        raise NotImplementedError()


@block_scope_owner
def enumerate():
    def handle_block_contents(contents: List):
        return DisplayList(mode="enumerate", items=contents)
    return handle_block_contents

@inline_scope_owner
def item():
    # TODO I feel iffy about an inline scope owner returning "paragraph"
    # Should put something in the inline_scope_owner decorator to check?
    def inner(sentence):
        return Paragraph(sentence)
    return inner

@dataclass
class Formatted:
    format_type: str # e.g. "emph"
    items: List

    def render(self):
        # TODO return "\" + self.format_type + "{" + render(self.items) + "}"
        raise NotImplementedError()

@inline_scope_owner
def emph():
    def inner(sentence):
        return Formatted("emph", sentence)
    return inner

@inline_scope_owner
def enquote():
    def inner(sentence):
        return ["``"] + sentence + ["''"]
    return inner

if __name__ == '__main__':
    CITATIONS = {} # load_cites("phdprop.bibtex")
    doc_block = parse_file("./examples/phdprop.ttxt", locals=locals())

