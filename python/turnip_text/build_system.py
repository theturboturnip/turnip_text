"""Generating a document requires reading from the main document source code and possibly supplementary files, and writing out a new document file and possibly other supplementary files e.g. a bibliography, diagrams in the right format, etc.

Ideally this process can be tested, hence a minimal file system abstraction is required to track the involved files.
"""

import abc
import io
import pathlib
import re
import tempfile
from typing import (
    Callable,
    ContextManager,
    Dict,
    List,
    Set,
    Tuple,
    TypeAlias,
    Union,
    overload,
)

from typing_extensions import override

from turnip_text import TurnipTextSource

InputRelPath = Union[str, "RelPath"]
OutputRelPath = Union[str, "RelPath"]


class BuildSystem:
    _file_system: "Filesystem"
    _deferred_supplementary_file_jobs: List[Callable[["BuildSystem"], None]]

    def __init__(
        self, input_dir: pathlib.Path, output_dir: Union[pathlib.Path, "FileProvider"]
    ) -> None:
        super().__init__()
        self._file_system = Filesystem(
            {
                "input": RealFileSystemProvider(input_dir, mkdirs=False),
                "temp": TempBackedFileProvider(),
                "output": (
                    output_dir
                    if isinstance(output_dir, FileProvider)
                    else RealFileSystemProvider(output_dir, mkdirs=True)
                ),
            }
        )
        self._deferred_supplementary_file_jobs = []

    def resolve_turnip_text_source(
        self, rel_path: InputRelPath, encoding: str = "utf-8"
    ) -> TurnipTextSource:
        file = self._file_system.resolve_input_file(RelPath("input", rel_path))
        with file.open_read_text(encoding=encoding) as f:
            return TurnipTextSource(name=str(rel_path), contents=f.read())

    def resolve_input_file(self, rel_path: InputRelPath) -> "InputFile":
        try:
            return self._file_system.resolve_input_file(RelPath("input", rel_path))
        except:
            raise RuntimeError(f"Failed to resolve input file 'input/{rel_path}'")

    def resolve_temp_file(self, rel_path: Union[str, "RelPath"]) -> "OutputFile":
        try:
            return self._file_system.resolve_output_file(RelPath("temp", rel_path))
        except:
            raise RuntimeError(f"Failed to resolve temp file 'temp/{rel_path}'")

    def resolve_output_file(self, rel_path: OutputRelPath) -> "OutputFile":
        try:
            return self._file_system.resolve_output_file(RelPath("output", rel_path))
        except:
            raise RuntimeError(f"Failed to resolve output file 'output/{rel_path}'")

    def defer_supplementary_file(self, job: Callable[["BuildSystem"], None]) -> None:
        self._deferred_supplementary_file_jobs.append(job)

    def run_deferred_jobs(self) -> None:
        for job in self._deferred_supplementary_file_jobs:
            job(self)
        self._deferred_supplementary_file_jobs = []

    def close(self) -> None:
        self._file_system.close()


RelPathComponent = str

# Invalid component = [~, more than two ., any string that isn't [a-zA-Z0-9\-_\.]+]
# i.e. strings of more than two dots, or any string with a character that isn't a-zA-Z...
INVALID_COMP_RE = re.compile(r"(^\.\.\.+$)|([^a-zA-Z0-9\-_\.])")


def check_component(component: str) -> RelPathComponent:
    if INVALID_COMP_RE.search(component):
        raise ValueError(
            f"{component} is not a valid relative-path component - must only be alphanumeric characters, underscores, dashes, and dots, and cannot be a string of 3+ dots."
        )
    return component


class RelPath:
    """
    A path that is always relative to some wider filesystem,
    and that automatically resolves relative components like '.' and '..'
    (under the assumption the filesystem doesn't use symlinks).

    Usable as a hash key.
    """

    components: Tuple[RelPathComponent, ...]

    def __init__(self, *to_join: Union[str, "RelPath"]) -> None:
        """
        Make a project- or output-relative-path.

        Each argument can be a UNIX-ish path, made of components separated with `/`.
        Leading slashes in each component are ignored.
        Components of `.` are equivalent to "the current directory" and are ignored.
        Components of `..` are equivalent to "the previous directory" and negate the previous component - unless there is no previous component, at which point an error is thrown.
        Components like `~` and `...`, `....` etc raise ValueError.

        Right now only alphanumeric characters, underscores, dots, and dashes are accepted in components.
        Whitespace is not allowed.
        These restrictions may be loosened later.
        """
        components_with_dots: List[str] = []
        for item in to_join:
            if isinstance(item, RelPath):
                components_with_dots.extend(item.components)
            else:
                # Remove leading and trailing /, then split on /
                components_with_dots.extend(item.strip("/").split("/"))

        components: List[RelPathComponent] = []
        for component in components_with_dots:
            if component == ".":
                continue
            elif component == "..":
                if components:
                    components.pop()
                else:
                    raise ValueError(
                        f"Tried to pop too far out of the relative path!\n{components_with_dots}"
                    )
            else:
                components.append(check_component(component))
        self.components = tuple(components)

    def __str__(self) -> str:
        return "/".join(self.components)

    def __eq__(self, __value: object) -> bool:
        if not isinstance(__value, RelPath):
            return False
        return self.components == __value.components

    def __hash__(self) -> int:
        return hash(self.components)

    def __bool__(self) -> bool:
        return bool(self.components)

    @overload
    def __getitem__(self, index: int) -> RelPathComponent: ...
    @overload
    def __getitem__(self, index: slice) -> "RelPath": ...
    def __getitem__(
        self, index: Union[int, slice]
    ) -> Union[RelPathComponent, "RelPath"]:
        if isinstance(index, slice):
            return RelPath(*self.components[index])
        return self.components[index]


ByteReader: TypeAlias = io.BufferedIOBase
ByteWriter: TypeAlias = io.BufferedIOBase
TextReader: TypeAlias = io.TextIOBase
TextWriter: TypeAlias = io.TextIOBase


class JobFile(abc.ABC):
    """
    A virtual file backed by a real file somewhere in this filesystem.
    The real file may be in a temporary directory or a permanent one, but it is accessible nonetheless.

    It must be backed by a filesystem so command-line tools like Pandoc can use it.
    """

    _path: pathlib.Path

    def __init__(
        self,
        path: pathlib.Path,
    ) -> None:
        super().__init__()
        self._path = path

    @property
    def external_path(self) -> pathlib.Path:
        """
        The resolved path that refers to this file in the real/external filesystem.

        This path is absolute, and can be used with open() and other filesystem functions directly.
        """
        return self._path


# TODO accept all other params to open()?
class InputFile(JobFile, abc.ABC):
    @abc.abstractmethod
    def open_read_bin(self) -> ContextManager[ByteReader]: ...

    @abc.abstractmethod
    def open_read_text(self, encoding: str = "utf-8") -> ContextManager[TextReader]: ...


class OutputFile(JobFile, abc.ABC):
    @abc.abstractmethod
    def open_write_bin(self) -> ContextManager[ByteWriter]: ...

    @abc.abstractmethod
    def open_write_text(
        self, encoding: str = "utf-8"
    ) -> ContextManager[TextWriter]: ...


class FileProvider(abc.ABC):
    @abc.abstractmethod
    def resolve_input_file(self, rel_path: RelPath) -> InputFile: ...

    @abc.abstractmethod
    def resolve_output_file(self, rel_path: RelPath) -> OutputFile: ...

    @abc.abstractmethod
    def close(self) -> None: ...


class Filesystem(FileProvider):
    bindings: Dict[str, FileProvider]

    def __init__(self, bindings: Dict[str, FileProvider]) -> None:
        for key in bindings:
            check_component(key)
        self.bindings = bindings

    @override
    def resolve_input_file(self, rel_path: RelPath) -> InputFile:
        if not rel_path:
            raise ValueError("Can't resolve empty path")
        if len(rel_path.components) == 1:
            raise ValueError(f"{rel_path} resolves to a folder, not a file")
        if rel_path[0] not in self.bindings:
            raise ValueError(f"{rel_path} does not exist")
        return self.bindings[rel_path[0]].resolve_input_file(rel_path[1:])

    @override
    def resolve_output_file(self, rel_path: RelPath) -> OutputFile:
        if not rel_path:
            raise ValueError("Can't resolve empty path")
        if len(rel_path.components) == 1:
            raise ValueError(f"{rel_path} resolves to a folder, not a file")
        if rel_path[0] not in self.bindings:
            raise ValueError(f"{rel_path} does not exist")
        return self.bindings[rel_path[0]].resolve_output_file(rel_path[1:])

    @override
    def close(self) -> None:
        for binding in self.bindings.values():
            binding.close()
        self.bindings = {}


class RealFileSystemProvider(FileProvider):
    base_path: pathlib.Path
    """The base directory for the filesystem"""
    mkdirs: bool
    """Are directories for sub-paths constructed on demand?"""

    def __init__(self, base_path: pathlib.Path, mkdirs: bool) -> None:
        super().__init__()
        assert (
            base_path.is_dir()
        ), "Can't create a RealFileSystem without a backing folder"
        self.base_path = base_path
        self.mkdirs = mkdirs

    @override
    def resolve_input_file(self, rel_path: RelPath) -> InputFile:
        path = self.base_path / str(rel_path)
        if self.mkdirs:
            path.parent.mkdir(parents=True, exist_ok=True)
        if not path.exists():
            raise ValueError(f"Cannot resolve nonexistant input file {rel_path}")
        return RealJobInputFile(path.resolve())

    @override
    def resolve_output_file(self, rel_path: RelPath) -> OutputFile:
        path = self.base_path / str(rel_path)
        if self.mkdirs:
            path.parent.mkdir(parents=True, exist_ok=True)
        return RealJobOutputFile(path.resolve())

    @override
    def close(self) -> None:
        pass


class RealJobInputFile(InputFile):
    """Implementation of JobInputFile for "real" files that exist in an external filesystem"""

    @override
    def open_read_bin(self) -> ContextManager[ByteReader]:
        return open(self._path, "rb")

    @override
    def open_read_text(self, encoding: str = "utf-8") -> ContextManager[TextReader]:
        return open(self._path, "r", encoding=encoding)


class RealJobOutputFile(OutputFile):
    """Implementation of JobOutputFile for "real" files that exist in an external filesystem"""

    @override
    def open_write_bin(self) -> ContextManager[ByteWriter]:
        return open(self._path, "wb")

    @override
    def open_write_text(self, encoding: str = "utf-8") -> ContextManager[TextWriter]:
        return open(self._path, "w", encoding=encoding)


class TempBackedFileProvider(FileProvider):
    """
    A FileProvider that creates temp files to back all requested files,
    and then loads the files into memory before finishing.

    Used if in-memory access to files is useful (e.g. for testing purposes)
    but real files are required for some tools like Pandoc.
    """

    temp_dir: tempfile.TemporaryDirectory[str]
    temp_dir_path: pathlib.Path
    resolver: RealFileSystemProvider
    files: Set[RelPath]

    def __init__(self) -> None:
        super().__init__()
        self.temp_dir = tempfile.TemporaryDirectory()
        self.temp_dir_path = pathlib.Path(self.temp_dir.name)
        self.resolver = RealFileSystemProvider(self.temp_dir_path, mkdirs=True)
        self.files = set()

    @override
    def resolve_input_file(self, rel_path: RelPath) -> InputFile:
        self.files.add(rel_path)
        return self.resolver.resolve_input_file(rel_path)

    @override
    def resolve_output_file(self, rel_path: RelPath) -> OutputFile:
        self.files.add(rel_path)
        return self.resolver.resolve_output_file(rel_path)

    def extract_contents(self) -> Dict[RelPath, bytes]:
        contents = {}
        for relpath in self.files:
            with open(self.temp_dir_path / str(relpath), "rb") as f:
                contents[relpath] = f.read()
        return contents

    @override
    def close(self) -> None:
        for file in self.files:
            (self.temp_dir_path / str(file)).unlink()
        self.files = set()
        # Make sure we don't try to use the temp dir again
        self.temp_dir_path = None  # type:ignore
        self.temp_dir = None  # type:ignore
