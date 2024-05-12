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
from typing import (
    Callable,
    ContextManager,
    Dict,
    Generator,
    List,
    Optional,
    Tuple,
    TypeAlias,
)

from turnip_text import TurnipTextSource, open_turnip_text_source

# TODO right now this doesn't handle ../s. That's probably a good thing.
ProjectRelativePath = str
OutputRelativePath = str
ResolvedPath = Path


ByteReader: TypeAlias = io.BufferedIOBase
ByteWriter: TypeAlias = io.BufferedIOBase
TextReader: TypeAlias = io.TextIOBase
TextWriter: TypeAlias = io.TextIOBase

# TODO build system should provide a temp output directory where all files have real paths
# Sometimes e.g. pandoc wants to use resources temporarily to generate an output document, but they don't need to be in the output folder.
# TODO build system should provide wildcard input restriction to a job
# e.g. this job must run last
# good for the above


class JobFile(abc.ABC):
    @abc.abstractproperty
    def external_path(self) -> Optional[ResolvedPath]:
        """The resolved path that refers to this file in the real/external filesystem - i.e. this path can be used with open() directly.

        May be None if the file cannot be opened through open() e.g. if provided by an in-memory filesystem.
        """
        ...


# TODO accept all other params to open()?
class JobInputFile(JobFile, abc.ABC):
    @property
    @abc.abstractmethod
    def path(self) -> ProjectRelativePath:
        """The internal path of this file. Always exists, not usable with filesystem functions."""
        ...

    # TODO define how multiple-reader is handled
    @abc.abstractmethod
    def open_read_bin(self) -> ContextManager[ByteReader]: ...

    @abc.abstractmethod
    def open_read_text(self, encoding: str = "utf-8") -> ContextManager[TextReader]: ...


class JobOutputFile(JobFile, abc.ABC):
    @property
    @abc.abstractmethod
    def path(self) -> OutputRelativePath:
        """The internal path of this file. Always exists, not usable with filesystem functions."""
        ...

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
    """A BuildSystem manages Jobs which take input files, process them, and save results to output files.
    It is also an abstraction over a potentially-virtual file system."""

    file_jobs: Dict[OutputRelativePath, Tuple[Dict[str, ProjectRelativePath], FileJob]]

    def __init__(self) -> None:
        super().__init__()
        self.file_jobs = {}

    @abc.abstractmethod
    def resolve_turnip_text_source(
        self, project_relative_path: ProjectRelativePath, encoding: str = "utf-8"
    ) -> TurnipTextSource: ...

    @abc.abstractmethod
    def resolve_input_file(
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

    def run_jobs(self) -> None:
        for output_relative_path, (relative_inputs, job) in self.file_jobs.items():
            out_file = self._resolve_output_file(output_relative_path)
            in_files = {
                name: self.resolve_input_file(project_relative_path)
                for name, project_relative_path in relative_inputs.items()
            }
            job(in_files, out_file)

    def clear_jobs(self) -> None:
        self.file_jobs.clear()


class SimpleBuildSystem(BuildSystem):
    project_dir: Path
    output_dir: Path

    def __init__(
        self, project_dir: Path, output_dir: Path, make_output_dir: bool = True
    ) -> None:
        super().__init__()
        project_dir = project_dir.resolve()
        if not project_dir.is_dir():
            raise ValueError(
                f"Project dir '{project_dir}' either doesn't exist or isn't a directory"
            )
        output_dir = output_dir.resolve()
        if not output_dir.is_dir():
            if make_output_dir:
                output_dir.mkdir(parents=True, exist_ok=True)
            else:
                raise ValueError(
                    f"Output dir '{output_dir}' either doesn't exist or isn't a directory"
                )
        self.project_dir = project_dir
        self.output_dir = output_dir

    # TODO we need some way to make sure the relative paths don't jump out of the project_dirs.

    def _resolve_project_relpath(
        self, project_relative_path: ProjectRelativePath
    ) -> ResolvedPath:
        p = self.project_dir / Path(project_relative_path)
        if not p.is_file():
            raise ValueError(
                f"Requested input '{project_relative_path}' doesn't exist in {self.project_dir} or is a directory"
            )
        return p.resolve()

    def _resolve_output_relpath(
        self, output_relative_path: OutputRelativePath
    ) -> ResolvedPath:
        p = (self.output_dir / Path(output_relative_path)).resolve()
        if not p.parent.exists():
            # TODO if we can verify we're within bounds, create the output directory.
            raise ValueError(
                f"Requested output '{output_relative_path}' doesn't have an existing parent directory '{p.parent}'."
            )
        return p

    def resolve_turnip_text_source(
        self, project_relative_path: ProjectRelativePath, encoding: str = "utf-8"
    ) -> TurnipTextSource:
        return open_turnip_text_source(
            str(self._resolve_project_relpath(project_relative_path)), encoding=encoding
        )

    def resolve_input_file(
        self, project_relative_path: ProjectRelativePath
    ) -> JobInputFile:
        return RealJobInputFile(
            self._resolve_project_relpath(project_relative_path),
            relpath=project_relative_path,
        )

    def _resolve_output_file(
        self, output_relative_path: OutputRelativePath
    ) -> JobOutputFile:
        return RealJobOutputFile(
            self._resolve_output_relpath(output_relative_path),
            relpath=output_relative_path,
        )


class RealJobInputFile(JobInputFile):
    """Implementation of JobInputFile for "real" files that exist in an external filesystem"""

    _relpath: ProjectRelativePath
    _path: Path

    def __init__(self, path: Path, relpath: ProjectRelativePath) -> None:
        super().__init__()
        self._path = path
        self._relpath = relpath

    @property
    def path(self) -> ProjectRelativePath:
        return self._relpath

    @property
    def external_path(self) -> Optional[ResolvedPath]:
        return self._path

    def open_read_bin(self) -> ContextManager[ByteReader]:
        return open(self._path, "rb")

    def open_read_text(self, encoding: str = "utf-8") -> ContextManager[TextReader]:
        return open(self._path, "r", encoding=encoding)


class RealJobOutputFile(JobOutputFile):
    """Implementation of JobOutputFile for "real" files that exist in an external filesystem"""

    _relpath: OutputRelativePath
    _path: Path

    def __init__(self, path: Path, relpath: OutputRelativePath) -> None:
        super().__init__()
        self._path = path
        self._relpath = relpath

    @property
    def path(self) -> OutputRelativePath:
        return self._relpath

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

    def resolve_turnip_text_source(
        self, project_relative_path: str, encoding: str = "utf-8"
    ) -> TurnipTextSource:
        data = self.input_files.get(project_relative_path)
        if data:
            return TurnipTextSource(project_relative_path, data.decode(encoding))
        raise ValueError(f"Input file '{project_relative_path}' doesn't exist")

    def resolve_input_file(self, project_relative_path: str) -> JobInputFile:
        data = self.input_files.get(project_relative_path)
        if data:
            return InMemoryInputFile(project_relative_path, data)
        raise ValueError(f"Input file '{project_relative_path}' doesn't exist")

    def _resolve_output_file(self, output_relative_path: str) -> JobOutputFile:
        # Creates and returns a usable file for an in-memory file system
        f = self.output_files.get(output_relative_path)
        if f:
            return f
        f = InMemoryOutputFile(output_relative_path)
        self.output_files[output_relative_path] = f
        return f

    def get_outputs(self) -> Dict[str, bytes]:
        return {k: v._bytes_writer.getvalue() for k, v in self.output_files.items()}


class InMemoryInputFile(JobInputFile):
    """Implementation of JobInputFile for a file from an in-memory filesystem"""

    _relpath: ProjectRelativePath
    _data: bytes

    def __init__(self, relpath: ProjectRelativePath, data: bytes) -> None:
        super().__init__()
        self._relpath = relpath
        self._data = data

    @property
    def path(self) -> ProjectRelativePath:
        return self._relpath

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
    wrapper = io.TextIOWrapper(bytes_writer, encoding=encoding)
    try:
        yield wrapper
    finally:
        # DON'T close the wrapper or let it be garbage collected because then it would close bytes_writer
        buf = wrapper.detach()
        assert not buf.closed
        pass


class InMemoryOutputFile(JobOutputFile):
    """Implementation of JobInputFile for a file from an in-memory filesystem"""

    _relpath: OutputRelativePath
    _bytes_writer: io.BytesIO

    def __init__(self, relpath: OutputRelativePath) -> None:
        super().__init__()
        self._relpath = relpath
        self._bytes_writer = io.BytesIO()

    @property
    def path(self) -> OutputRelativePath:
        return self._relpath

    @property
    def external_path(self) -> None:
        return None

    def open_write_bin(self) -> ContextManager[ByteWriter]:
        return non_closing_byte_writer(self._bytes_writer)

    def open_write_text(self, encoding: str = "utf-8") -> ContextManager[TextWriter]:
        return non_closing_text_wrapper(self._bytes_writer, encoding)


class StackBuildSystem(BuildSystem):
    """Implementation of BuildSystem that works as a 'stack' of other build systems.
    The internal build systems do not have jobs enqueued, but whenever an input or output file is resolved each build system is queried in order. The first one to not throw an error actually resolves the file.
    """

    _build_systems: List[BuildSystem]

    def __init__(self, build_systems: List[BuildSystem]) -> None:
        super().__init__()
        self._build_systems = build_systems

    def resolve_turnip_text_source(
        self,
        project_relative_path: str,
        encoding: str = "utf-8",
    ) -> TurnipTextSource:
        for b in self._build_systems:
            try:
                return b.resolve_turnip_text_source(project_relative_path, encoding)
            except:
                continue
        raise ValueError(
            f"None of the supplementary build systems had '{project_relative_path}'"
        )

    def resolve_input_file(self, project_relative_path: str) -> JobInputFile:
        for b in self._build_systems:
            try:
                return b.resolve_input_file(project_relative_path)
            except:
                continue
        raise ValueError(
            f"None of the supplementary build systems had '{project_relative_path}'"
        )

    def _resolve_output_file(self, output_relative_path: str) -> JobOutputFile:
        for b in self._build_systems:
            try:
                return b._resolve_output_file(output_relative_path)
            except:
                continue
        raise ValueError(
            f"None of the supplementary build systems could make the output file '{output_relative_path}'"
        )


class SplitBuildSystem(BuildSystem):
    """Implementation of a BuildSystem that combines separate BuildSystems for input and output files.
    The internal build systems do not have jobs enqueued, only this top-level one.
    This allows e.g. input files to come from a real filesystem and output files to go to an in-memory filesystem.
    """

    _input_build_sys: BuildSystem
    _output_build_sys: BuildSystem

    def __init__(
        self, input_build_sys: BuildSystem, output_build_sys: BuildSystem
    ) -> None:
        super().__init__()
        self._input_build_sys = input_build_sys
        self._output_build_sys = output_build_sys

    def resolve_turnip_text_source(
        self,
        project_relative_path: str,
        encoding: str = "utf-8",
    ) -> TurnipTextSource:
        return self._input_build_sys.resolve_turnip_text_source(
            project_relative_path, encoding
        )

    def resolve_input_file(self, project_relative_path: str) -> JobInputFile:
        return self._input_build_sys.resolve_input_file(project_relative_path)

    def _resolve_output_file(self, output_relative_path: str) -> JobOutputFile:
        return self._output_build_sys._resolve_output_file(output_relative_path)
