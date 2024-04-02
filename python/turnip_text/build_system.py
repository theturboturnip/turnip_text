"""Generating a document requires reading from the main document source code and possibly supplementary files, and writing out a new document file and possibly other supplementary files e.g. a bibliography, diagrams in the right format, etc.

This requires a (minimal) build system to track the supplementary files and where they should be opened from + saved.

This build system does not support dependencies or e.g. skipping tasks based on not-modified files.
Dependencies are unnecessary as these jobs do not mutate program state, they just push data into output files.

The BuildSystem considers project-relative paths for input files and output-relative paths for output files.
In the simple case these are relative to a single project folder and output folder respectively, but there may be room for the BuildSystem to transparently remap them into different folders later down the line.

Indeed, the BuildSystem may use Path under the hood to identify files, but the JobFile instances it passes to the Jobs are an abstraction that do not need to map to files in a "real" filesystem.
JobInputFiles provide open_read_{bin,text}() and JobOutputFiles provide open_write_{bin,text}() to open read/write handles to files regardless of where they are.
This allows unit tests etc. to use in-memory filesystems without changing Job code.
If a Job absolutely needs an external filesystem path to a file (for example to pass it through external programs) then it may request the path, but if the file is in-memory it may not have one.
"""

import abc
import io
from contextlib import contextmanager
from pathlib import Path
from typing import Callable, ContextManager, Dict, Generator, Optional, Tuple, TypeAlias

from turnip_text import InsertedFile

# TODO right now this doesn't handle ../s. That's probably a good thing.
ProjectRelativePath = str
OutputRelativePath = str
ResolvedPath = Path


ByteReader: TypeAlias = io.BufferedIOBase
ByteWriter: TypeAlias = io.BufferedIOBase
TextReader: TypeAlias = io.TextIOBase
TextWriter: TypeAlias = io.TextIOBase


class JobFile(abc.ABC):
    @abc.abstractproperty
    def external_path(self) -> Optional[ResolvedPath]:
        """The resolved path that refers to this file in the real/external filesystem - i.e. this path can be used with open() directly.

        May be None if the file cannot be opened through open() e.g. if provided by an in-memory filesystem.
        """
        ...


# TODO accept all other params to open()?
class JobInputFile(JobFile, abc.ABC):
    # TODO define how multiple-reader is handled
    @abc.abstractmethod
    def open_read_bin(self) -> ContextManager[ByteReader]: ...

    @abc.abstractmethod
    def open_read_text(self, encoding: str = "utf-8") -> ContextManager[TextReader]: ...


class JobOutputFile(JobFile, abc.ABC):
    # TODO define how multiple-writer is handled.
    @abc.abstractmethod
    def open_write_bin(self) -> ContextManager[ByteWriter]: ...

    @abc.abstractmethod
    def open_write_text(
        self, encoding: str = "utf-8"
    ) -> ContextManager[TextWriter]: ...


FileJobInputs = Dict[str, JobInputFile]

# A FileJob is a function the BuildSystem eventually calls with the resolved path to an output file.
FileJob = Callable[[FileJobInputs, JobOutputFile], None]


class BuildSystem(abc.ABC):
    file_jobs: Dict[OutputRelativePath, Tuple[Dict[str, ProjectRelativePath], FileJob]]

    def __init__(self) -> None:
        super().__init__()
        self.file_jobs = {}

    @abc.abstractmethod
    def resolve_turnip_text_source(
        self, project_relative_path: ProjectRelativePath
    ) -> InsertedFile: ...

    @abc.abstractmethod
    def _resolve_input_file(
        self, project_relative_path: ProjectRelativePath
    ) -> JobInputFile: ...

    @abc.abstractmethod
    def _resolve_output_file(
        self, output_relative_path: OutputRelativePath
    ) -> JobOutputFile: ...

    def register_file_generator(
        self,
        job: FileJob,
        inputs: Dict[str, ProjectRelativePath],
        output_relative_path: OutputRelativePath,
    ) -> None:
        """Track a job and an output-relative path that will eventually be populated with data by the job"""

        if output_relative_path in self.file_jobs:
            raise ValueError(
                f"Two jobs tried to generate the same overall file {output_relative_path}"
            )

        self.file_jobs[output_relative_path] = (inputs, job)


class SimpleBuildSystem(BuildSystem):
    project_dir: Path
    output_dir: Path

    def __init__(self, project_dir: Path, output_dir: Path) -> None:
        super().__init__()
        project_dir = project_dir.resolve()
        if not project_dir.is_dir():
            raise ValueError(
                f"Project dir '{project_dir}' either doesn't exist or isn't a directory"
            )
        output_dir = output_dir.resolve()
        if not output_dir.is_dir():
            raise ValueError(
                f"Output dir '{output_dir}' either doesn't exist or isn't a directory"
            )
        self.project_dir = project_dir
        self.output_dir = output_dir

    def resolve_turnip_text_source(
        self, project_relative_path: ProjectRelativePath
    ) -> InsertedFile:
        return InsertedFile.from_path(
            str((self.project_dir / Path(project_relative_path)).resolve())
        )

    def _resolve_input_file(
        self, project_relative_path: ProjectRelativePath
    ) -> JobInputFile:
        # TODO check if the project_relative_path file really exists
        return RealJobInputFile(
            (self.project_dir / Path(project_relative_path)).resolve()
        )

    def _resolve_output_file(
        self, output_relative_path: OutputRelativePath
    ) -> JobOutputFile:
        # TODO make sure the directory for output_relative_path exists
        return RealJobOutputFile(
            (self.output_dir / Path(output_relative_path)).resolve()
        )


class RealJobInputFile(JobInputFile):
    """Implementation of JobInputFile for "real" files that exist in an external filesystem"""

    _path: Path

    def __init__(self, path: Path) -> None:
        super().__init__()
        self._path = path

    @property
    def external_path(self) -> Optional[ResolvedPath]:
        return self._path

    def open_read_bin(self) -> ContextManager[ByteReader]:
        return open(self._path, "rb")

    def open_read_text(self, encoding: str = "utf-8") -> ContextManager[TextReader]:
        return open(self._path, "r", encoding=encoding)


class RealJobOutputFile(JobOutputFile):
    """Implementation of JobOutputFile for "real" files that exist in an external filesystem"""

    _path: Path

    def __init__(self, path: Path) -> None:
        super().__init__()
        self._path = path

    @property
    def external_path(self) -> Optional[ResolvedPath]:
        return self._path

    def open_write_bin(self) -> ContextManager[ByteWriter]:
        return open(self._path, "wb")

    def open_write_text(self, encoding: str = "utf-8") -> ContextManager[TextWriter]:
        return open(self._path, "w", encoding=encoding)


class InMemoryBuildSystem(BuildSystem):
    """Implementation of BuildSystem that uses in-memory streams (BytesIO/StringIO) instead of real files."""

    input_files: Dict[str, bytes]
    output_files: Dict[str, "InMemoryOutputFile"]

    def __init__(self, input_files: Dict[str, bytes]) -> None:
        super().__init__()
        self.input_files = input_files
        self.output_files = {}

    def resolve_turnip_text_source(self, project_relative_path: str) -> InsertedFile:
        data = self.input_files.get(project_relative_path)
        if data:
            # TODO - make it possible to insert a custom "path" into InsertedFile
            return InsertedFile.from_string(data.decode("utf-8"))
        raise ValueError(f"Input file '{project_relative_path}' doesn't exist")

    def _resolve_input_file(self, project_relative_path: str) -> JobInputFile:
        data = self.input_files.get(project_relative_path)
        if data:
            return InMemoryInputFile(data)
        raise ValueError(f"Input file '{project_relative_path}' doesn't exist")

    def _resolve_output_file(self, output_relative_path: str) -> JobOutputFile:
        # Creates and returns a usable file for an in-memory file system
        f = self.output_files.get(output_relative_path)
        if f:
            return f
        f = InMemoryOutputFile()
        self.output_files[output_relative_path] = f
        return f

    def get_outputs(self) -> Dict[str, bytes]:
        return {k: v._bytes_writer.getvalue() for k, v in self.output_files.items()}


class InMemoryInputFile(JobInputFile):
    """Implementation of JobInputFile for a file from an in-memory filesystem"""

    _data: bytes

    def __init__(self, data: bytes) -> None:
        super().__init__()
        self._data = data

    @property
    def external_path(self) -> None:
        return None

    def open_read_bin(self) -> ContextManager[ByteReader]:
        return io.BytesIO(initial_bytes=self._data)

    def open_read_text(self, encoding: str = "utf-8") -> ContextManager[TextReader]:
        return io.StringIO(initial_value=self._data.decode(encoding=encoding))


@contextmanager
def non_closing_byte_writer(
    bytes_writer: io.BytesIO,
) -> Generator[ByteWriter, None, None]:
    try:
        yield bytes_writer
    finally:
        pass  # DON'T close bytes_writer


@contextmanager
def non_closing_text_wrapper(
    bytes_writer: io.BytesIO,
    encoding: str,
) -> Generator[TextWriter, None, None]:
    try:
        yield io.TextIOWrapper(bytes_writer, encoding=encoding)
    finally:
        pass  # DON'T close it because then it would close bytes_writer (I think?)


class InMemoryOutputFile(JobOutputFile):
    """Implementation of JobInputFile for a file from an in-memory filesystem"""

    _bytes_writer: io.BytesIO

    def __init__(self) -> None:
        super().__init__()
        self._bytes_writer = io.BytesIO()

    @property
    def external_path(self) -> None:
        return None

    def open_write_bin(self) -> ContextManager[ByteWriter]:
        return non_closing_byte_writer(self._bytes_writer)

    def open_write_text(self, encoding: str = "utf-8") -> ContextManager[TextWriter]:
        return non_closing_text_wrapper(self._bytes_writer, encoding)
