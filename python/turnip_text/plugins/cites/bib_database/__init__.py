import abc

from turnip_text.build_system import TextWriter

"""
This library is intended to provide a consistent method for reducing bibliographies to the minimal required set for a given document (My_Whole_Library.bib -> This_Article.bib)

Later, it might be used as a metadata hub (e.g. \citeauthor equivalent in LaTeX?)
Other possible features:
Automatic sanitization? e.g. if DOI and URL choose DOI?
Automatic nice sorting?
"""


class CitationDB(abc.ABC):
    """
    This is an abstract base class for different formats of citation database (e.g. CSL JSON, BibLaTeX...)
    """

    def has_entry(self, id: str) -> bool:
        raise NotImplementedError()

    def register_entry_used(self, id: str) -> None:
        """Should raise ValueError if id not present"""
        raise NotImplementedError()

    def write_minimal_db(self, io: TextWriter) -> None:
        """Write a citation DB of this format to a text-based IO channel, including only items with IDs that were registered via self.register_entry_used()"""
        raise NotImplementedError()
