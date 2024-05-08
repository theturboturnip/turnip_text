import json
from pathlib import Path
from typing import Any, Dict, List, Set, TextIO

from turnip_text.build_system import BuildSystem, ProjectRelativePath, TextWriter
from turnip_text.plugins.cites.bib_database import CitationDB


class CslJsonCitationDB(CitationDB):
    FILE_EXT = ".json"

    # Should write_minimal_db use a pretty-print JSON export?
    pretty_print_minimal_db: bool

    # Mapping of id -> dictionary
    csl_json_entries: Dict[str, Dict[Any, Any]]

    # List of used entries
    used_entries: Set[str]

    def __init__(
        self,
        file_sys: BuildSystem,
        paths: List[ProjectRelativePath],
        pretty_print_minimal_db: bool = True,
    ) -> None:

        self.pretty_print_minimal_db = pretty_print_minimal_db

        self.csl_json_entries = {}
        for path in paths:
            with file_sys.resolve_input_file(path).open_read_text() as f:
                csl_json = json.load(f)
                if not isinstance(csl_json, list):
                    raise TypeError(
                        f"File {path} has invalid format - expected list at top level, got {type(csl_json)}"
                    )
                for obj in csl_json:
                    if (not isinstance(obj, dict)) or ("id" not in obj):
                        raise TypeError(
                            f"Entry in file {path} has invalid format - expected dictionary with 'id' key, got {obj}"
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

    def write_minimal_db(self, io: TextWriter) -> None:
        json.dump(
            [
                csl_entry
                for (csl_id, csl_entry) in self.csl_json_entries.items()
                if csl_id in self.used_entries
            ],
            io,
            indent=4 if self.pretty_print_minimal_db else 0,
        )
