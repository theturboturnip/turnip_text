import abc
import json
from pathlib import Path
from typing import Any, Dict, List, Set, TextIO

import bibtexparser  # type: ignore[import-untyped]

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

    FILE_EXT: str
    """
    The recommended file extension for a database of this format, preceded with a .
    Should be set by subclass!
    """

    paths: List[Path]
    """
    The list of paths which form the origin for the database.
    Guaranteed to be Path objects where p.is_file() returned True
    Set and checked in CitationDB.__init__()
    """

    def __init__(self, unchecked_paths: List[Path | str]) -> None:
        super().__init__()

        if self.FILE_EXT is None:
            raise TypeError(
                f"{self.__class__.__name__} class (subclass of CitationDB) doesn't set FILE_EXT"
            )

        self.paths = [p if isinstance(p, Path) else Path(p) for p in unchecked_paths]

        bad_paths = [str(p) for p in self.paths if not p.is_file()]
        if bad_paths:
            raise RuntimeError(
                f"Tried to construct citation database from nonexistent files {','.join(bad_paths)}"
            )

    def has_entry(self, id: str) -> bool:
        raise NotImplementedError()

    def register_entry_used(self, id: str) -> None:
        """Should raise ValueError if id not present"""
        raise NotImplementedError()

    def write_minimal_db(self, io: TextIO) -> None:
        """Write a citation DB of this format to a text-based IO channel, including only items with IDs that were registered via self.register_entry_used()"""
        raise NotImplementedError()


class CslJsonCitationDB(CitationDB):
    FILE_EXT = ".json"

    # Should write_minimal_db use a pretty-print JSON export?
    pretty_print_minimal_db: bool

    # Mapping of id -> dictionary
    csl_json_entries: Dict[str, Dict[str, Any]]

    # List of used entries
    used_entries: Set[str]

    def __init__(
        self, paths: List[Path | str], pretty_print_minimal_db: bool = True
    ) -> None:
        super().__init__(paths)

        self.pretty_print_minimal_db = pretty_print_minimal_db

        self.csl_json_entries = {}
        for p in self.paths:
            with open(p, "r", encoding="utf-8") as f:
                csl_json = json.load(f)
                if not isinstance(csl_json, list):
                    raise TypeError(
                        f"File {p} has invalid format - expected list at top level, got {type(csl_json)}"
                    )
                for obj in csl_json:
                    if (not isinstance(obj, dict)) or ("id" not in obj):
                        raise TypeError(
                            f"Entry in file {p} has invalid format - expected dictionary with 'id' key, got {obj}"
                        )
                    obj_id = str(obj["id"])
                    if obj_id in self.csl_json_entries:
                        raise RuntimeError(
                            f"Multiple-definition of citation ID '{obj_id}'"
                        )
                    self.csl_json_entries[obj_id] = obj

        self.used_entries = set()

    def has_entry(self, id: str) -> bool:
        return id in self.csl_json_entries

    def register_entry_used(self, id: str) -> None:
        if id not in self.csl_json_entries:
            raise ValueError(f"Citation ID {id} not present in database")
        self.used_entries.add(id)

    def write_minimal_db(self, io: TextIO) -> None:
        json.dump(
            [
                csl_entry
                for (csl_id, csl_entry) in self.csl_json_entries.items()
                if csl_id in self.used_entries
            ],
            io,
            indent=4 if self.pretty_print_minimal_db else 0,
        )


class BibLatexCitationDB(CitationDB):
    FILE_EXT = ".bib"

    known_citekeys: Set[str]

    dbs: List[bibtexparser.bibdatabase.BibDatabase]

    # List of used entries
    used_entries: Set[str]

    def __init__(self, paths: List[Path | str]) -> None:
        super().__init__(paths)

        parser = bibtexparser.bparser.BibTexParser(
            ignore_nonstandard_types=False,
            interpolate_strings=False,
            common_strings=False,
            add_missing_from_crossref=False,
        )

        # TODO parser.expect_multiple_parse

        self.known_citekeys = set()
        self.dbs = []
        for p in self.paths:
            with open(p, "r", encoding="utf-8") as f:
                db = bibtexparser.load(f, parser)

                if not db.entries:
                    raise RuntimeError(f"Citation file {p} has no BibLaTeX entries")

                mulitply_defined = self.known_citekeys.intersection(
                    db.entries_dict.keys()
                )
                if mulitply_defined:
                    raise RuntimeError(
                        f"Multiple-definition of citation IDs {mulitply_defined}"
                    )

                self.known_citekeys.update(db.entries_dict.keys())
                self.dbs.append(db)

        self.used_entries = set()

    def has_entry(self, id: str) -> bool:
        return id in self.known_citekeys

    def register_entry_used(self, id: str) -> None:
        if not self.has_entry(id):
            raise ValueError(f"Citation ID {id} not present in database")
        self.used_entries.add(id)

    def _generate_clean_entry(self, e: Dict[str, Any]) -> Dict[str, Any]:
        e = e.copy()
        if "file" in e:
            del e["file"]
        if "abstract" in e:
            del e["abstract"]
        return e

    def write_minimal_db(self, io: TextIO) -> None:
        minimal_db = bibtexparser.bibdatabase.BibDatabase()

        for db in self.dbs:
            minimal_db.comments.extend(db.comments)
            minimal_db.preambles.extend(db.preambles)
            minimal_db.strings.update(db.strings)
            minimal_db.entries.extend(
                self._generate_clean_entry(e)
                for e in db.entries
                if e["ID"] in self.used_entries
            )

        minimal_db.get_entry_dict()

        bibtexparser.dump(minimal_db, io)
