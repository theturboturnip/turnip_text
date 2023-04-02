from typing import Callable, Optional, Protocol, Tuple, Union, runtime_checkable

from turnip_text import BlockScopeBuilder, Inline, InlineScopeBuilder

CiteKey = str
CiteKeyWithNote = Tuple[CiteKey, str]


@runtime_checkable
class Renderer(Protocol):
    """
    Renderers should export a consistent set of functions for documents to rely on.
    """
    
    # Inline formatting
    emph: InlineScopeBuilder

    # Citations
    @staticmethod
    def cite(*keys: Union[CiteKey, CiteKeyWithNote]) -> Inline: ...
    @staticmethod
    def citeauthor(key: CiteKey) -> Inline: ...

    # Footnotes
    @property
    def footnote(self) -> InlineScopeBuilder:
        """Define an unlabelled footnote, capturing the following inline text as the contents,
        and insert a reference to that footnote here.
        
        `[footnote]{contents}` is roughly equivalent to `[footnote_ref(<some_label>)]` and `[footnote_text(<some_label>)]{contents}`"""
        ...
    @staticmethod
    def footnote_ref(label: str) -> Inline:
        """Insert a reference to a footnote, which will have the contents defined by a matching call to footnote_text.
        If, at render time, footnote_text has not been called with a matching label then an error will be raised."""
        ...
    @staticmethod
    def footnote_text(label: str) -> BlockScopeBuilder:
        """Define the contents of a footnote with the given label. Cannot be called twice with the same label."""
        ...

    # Structure
    @staticmethod
    def section(name: str, label: Optional[str] = None, numbered: bool = False) -> BlockScopeBuilder: ...
    @staticmethod
    def subsection(name: str, label: Optional[str] = None, numbered: bool = False) -> BlockScopeBuilder: ...
    @staticmethod
    def subsubsection(name: str, label: Optional[str] = None, numbered: bool = False) -> BlockScopeBuilder: ...

    # Figures?
