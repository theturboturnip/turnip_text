from typing import Optional, Protocol, Tuple, Union

from turnip_text import BlockScopeBuilder, Inline, InlineScopeBuilder

CiteKey = str
CiteKeyWithNote = Tuple[CiteKey, str]


class CitationPluginInterface(Protocol):
    """Standard plugin for citations"""

    _BIBLIOGRAPHY_POSTAMBLE_ID: str = "CitationPluginInterface_Bibliography"
    """Standard ID for the bibliography postamble"""

    def cite(self, *keys: Union[CiteKey, CiteKeyWithNote]) -> Inline:
        """Insert an inline citation for the references with the given keys, appending notes for each one"""
        ...

    def citeauthor(self, key: CiteKey) -> Inline:
        """Insert the name of the author(s) for the reference with the given key"""
        ...


class FootnotePluginInterface(Protocol):
    """Standard plugin for footnotes"""

    @property
    def footnote(self) -> InlineScopeBuilder:
        """Define an unlabelled footnote, capturing the following inline text as the contents,
        and insert a reference to that footnote here.

        `[footnote]{contents}` is roughly equivalent to `[footnote_ref(<some_label>)]` and `[footnote_text(<some_label>)]{contents}`
        """
        ...

    def footnote_ref(self, label: str) -> Inline:
        """Insert a reference to a footnote, which will have the contents defined by a matching call to footnote_text.
        If, at render time, footnote_text has not been called with a matching label then an error will be raised.
        """
        ...

    def footnote_text(self, label: str) -> BlockScopeBuilder:
        """Define the contents of a footnote with the given label. Cannot be called twice with the same label."""
        ...


class SectionPluginInterface(Protocol):
    def section(
        self, name: str, label: Optional[str] = None, numbered: bool = False
    ) -> BlockScopeBuilder:
        ...

    def subsection(
        self, name: str, label: Optional[str] = None, numbered: bool = False
    ) -> BlockScopeBuilder:
        ...

    def subsubsection(
        self, name: str, label: Optional[str] = None, numbered: bool = False
    ) -> BlockScopeBuilder:
        ...


class FormatPluginInterface(Protocol):
    emph: InlineScopeBuilder
    italic: InlineScopeBuilder
    bold: InlineScopeBuilder

    @property
    def enquote(self) -> InlineScopeBuilder:
        ...


class ListPluginInterface(Protocol):
    @property
    def enumerate(self) -> BlockScopeBuilder:
        ...

    @property
    def itemize(self) -> BlockScopeBuilder:
        ...

    @property
    def item(self) -> BlockScopeBuilder:
        ...


class UrlPluginInterface(Protocol):
    def url(self, url: str) -> Inline:
        ...
