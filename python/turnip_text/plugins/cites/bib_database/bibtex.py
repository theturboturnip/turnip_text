import pathlib
from typing import Dict, List, Set, TextIO, Tuple

import bibtexparser  # type: ignore

from turnip_text.build_system import BuildSystem, InputFile, InputRelPath, TextWriter
from turnip_text.plugins.cites.bib_database import CitationDB

# This library uses a pure Python BibLaTeX parser, which can be slow.
# If creating a lot of documents in one turnip_text run, it can be a huge time waste to parse the same bib over and over.
# Thus cache the results.

# TODO parser.expect_multiple_parse
BIBTEX_PARSER = bibtexparser.bparser.BibTexParser(
    ignore_nonstandard_types=False,
    interpolate_strings=False,
    common_strings=False,
    add_missing_from_crossref=False,
)
BIBTEX_CACHE: Dict[pathlib.Path, Tuple[str, bibtexparser.bibdatabase.BibDatabase]] = {}

def get_cached_db(rel_path: InputRelPath, file: InputFile) -> bibtexparser.bibdatabase.BibDatabase:
    with file.open_read_text() as f:
        file_contents = f.read()
    
    # if not None, it's (cached_contents: str, cached_db: BibDatabase)
    cached = BIBTEX_CACHE.get(file.external_path)
    # Just test if the strings are equal - CPython should do a length check first which would trivialize it in most cases,
    # and in the same-length-different-content case a hash would still need to read every character.
    if cached is not None and (cached[0] == file_contents):
        return cached[1]
    
    # Didn't have a cached version
    db = bibtexparser.loads(file_contents, BIBTEX_PARSER)

    if not db.entries:
        raise RuntimeError(
            f"Citation file '{rel_path}' has no BibLaTeX entries"
        )
    
    BIBTEX_CACHE[file.external_path] = (file_contents, db)
    return db


class BibLatexCitationDB(CitationDB):
    FILE_EXT = ".bib"

    known_citekeys: Set[str]

    dbs: List[bibtexparser.bibdatabase.BibDatabase]

    # List of used entries
    used_entries: Set[str]

    def __init__(self, file_sys: BuildSystem, paths: List[InputRelPath]) -> None:
        self.known_citekeys = set()
        self.dbs = []
        for path in paths:
            file = file_sys.resolve_input_file(path)
            db = get_cached_db(path, file)

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
