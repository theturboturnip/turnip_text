"""When generating a document you might need to generate supplementary files e.g. a bibliography, diagrams in the right format, etc.

This requires a (minimal) build system to track the supplementary files and where they should be saved.

This build system does not support dependencies or e.g. skipping tasks based on not-modified files.
Dependencies are unnecessary as these jobs do not mutate program state, they just push data into output files.

Every Renderer instance should take a BuildSystem as an arugment to __init__, although the RendererSetup that created it may restrict what types of BuildSystem are passed through for a given format.

The BuildSystem considers project-relative paths for input files and output-relative paths for output files.
In the simple case these are relative to a single project folder and output folder respectively, but there may be room for the BuildSystem to transparently remap them into different folders later down the line.

TODO it would be useful to rework this to support transparent remapping to in-memory file systems - instead of giving people Paths, give them opaque file handles directly?
TODO can't quite do that :/ because in some cases it may be delegating to shell scripts or external programs that need files??
"""

import abc
import io
from pathlib import Path
from typing import Callable, Dict, Set, Tuple

ProjectRelativePath = Path
OutputRelativePath = Path
ResolvedPath = Path

FileJobInputs = Dict[str, ResolvedPath]

# A FileJob is a function the BuildSystem eventually calls with the resolved path to an output file.
# It is the responsibility of the FileJob to open the file in the correct mode and produce the output.
FileJob = Callable[[FileJobInputs, ResolvedPath], None]


class BuildSystem(abc.ABC):
    file_jobs: Dict[ResolvedPath, Tuple[Dict[str, ProjectRelativePath], FileJob]]

    def __init__(self) -> None:
        super().__init__()
        self.file_jobs = {}

    @abc.abstractmethod
    def _resolve_project_relative_path(
        self, project_relative_path: ProjectRelativePath
    ) -> ResolvedPath: ...

    @abc.abstractmethod
    def _resolve_output_relative_path(
        self, output_relative_path: OutputRelativePath
    ) -> ResolvedPath: ...

    def register_file_generator(
        self,
        job: FileJob,
        inputs: Dict[str, ProjectRelativePath],
        output_relative_path: OutputRelativePath,
    ) -> None:
        """Track a job and an output-relative path that will eventually be populated with data by the job"""

        output_path = self._resolve_output_relative_path(output_relative_path).resolve()

        if output_path in self.file_jobs:
            raise ValueError(
                f"Two jobs tried to generate the same overall file {output_path}"
            )

        self.file_jobs[output_path] = (inputs, job)


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

    def _resolve_project_relative_path(
        self, project_relative_path: ProjectRelativePath
    ) -> ResolvedPath:
        return (self.project_dir / project_relative_path).resolve()

    def _aresolve_output_relative_path(
        self, output_relative_path: OutputRelativePath
    ) -> ResolvedPath:
        return (self.output_dir / output_relative_path).resolve()
