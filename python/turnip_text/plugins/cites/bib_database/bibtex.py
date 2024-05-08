from typing import List, Set, TextIO

import bibtexparser  # type: ignore

from turnip_text.build_system import BuildSystem, ProjectRelativePath, TextWriter
from turnip_text.plugins.cites.bib_database import CitationDB


class BibLatexCitationDB(CitationDB):
    FILE_EXT = ".bib"

    known_citekeys: Set[str]

    dbs: List[bibtexparser.bibdatabase.BibDatabase]

    # List of used entries
    used_entries: Set[str]

    def __init__(self, file_sys: BuildSystem, paths: List[ProjectRelativePath]) -> None:

        parser = bibtexparser.bparser.BibTexParser(
            ignore_nonstandard_types=False,
            interpolate_strings=False,
            common_strings=False,
            add_missing_from_crossref=False,
        )

        # TODO parser.expect_multiple_parse

        self.known_citekeys = set()
        self.dbs = []
        for path in paths:
            with file_sys.resolve_input_file(path).open_read_text() as f:
                db = bibtexparser.load(f, parser)

                if not db.entries:
                    raise RuntimeError(
                        f"Citation file '{path}' has no BibLaTeX entries"
                    )

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

    def _generate_clean_entry(self, e: dict) -> dict:  # type:ignore[type-arg]
        e = e.copy()
        if "file" in e:
            del e["file"]
        if "abstract" in e:
            del e["abstract"]
        return e

    def write_minimal_db(self, io: TextWriter) -> None:
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
