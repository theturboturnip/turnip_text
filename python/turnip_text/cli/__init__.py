import pathlib
from dataclasses import dataclass
from enum import IntEnum
from typing import Any, Dict, Generic, List, Optional, Tuple, Union

from turnip_text.build_system import (
    BuildSystem,
    RelPath,
    RelPathComponent,
    check_component,
)
from turnip_text.render import RenderPlugin, RenderSetup, TRenderSetup
from turnip_text.system import parse_and_emit


@dataclass
class GeneratedSetup(Generic[TRenderSetup]):
    render_setup: TRenderSetup
    plugins: List[RenderPlugin[TRenderSetup]]
    output_filename: str

class TurnipTextSetup:
    """
    A class which generates RenderSetups with a consistent plugin interface
    for different renderers.

    turnip_text files can request a TurnipTextSetup (indicating what plugins they use) through a shebang,
    which the turnip_text CLI will autodetect when trying to render the file.
    """

    DEFAULT_INPUTS: Dict[str, str] = {
        "input_stem": "dummy",
        "requested_format": "latex",
    }
    """
    A value that when passed to generate_setup() will always result in a valid RenderSetup and plugins.
    
    Used by the turnip_text CLI to provide documentation on available plugin functions.
    """

    def generate_setup(
        self,
        input_stem: str,
        requested_format: str,
        **kwargs: str,
    ) -> GeneratedSetup:
        """
        Given
        - a turnip_text suggested renderer code
        - a user-defined string

        return a RenderSetup and a list of plugins to use with that RenderSetup to render a turnip_text file.
        Otherwise, raise NotImplementedError.
        """
        raise NotImplementedError(
            f"{self.__class__.__name__} doesn't have a setup for suggested renderer '{requested_format}'"
        )


@dataclass
class InputParams:
    project_dir: pathlib.Path
    input_rel_path: RelPath


def autodetect_input(input_arg: str, project_folder_arg: Optional[str]) -> InputParams:
    """
    Given the required [input] argument and the optional [--project-dir] argument,
    determine or infer the what the project directory and input-relative-paths are

    If [--project-dir] is supplied:
    - If [input] is a valid file then input-relative-path is [input]-relative-to-[--project-dir]
    - Otherwise assume [input] is already an input-relative-path

    If [--project-dir] is not supplied:
    - If [input] is a relative path with more than one component, take the first component as [--project-dir]
        - e.g. ./examples/notes/file.ttext will infer (./examples/) (notes/file.ttext), which is useful as non-text resources like bibliographies usually are in sibling folders ./examples/bibs/bibliography.bib
    - If [input] is a relative path with exactly one component, take '.' as [--project-dir] and use input as a relative path
        - e.g. ./file.ttext will infer (.) (file.ttext)
    - Reject absolute paths and request [--project-dir] in that case
    """
    if project_folder_arg:
        project_dir = pathlib.Path(project_folder_arg)
        # Try the input_arg directly first
        input_path = pathlib.Path(input_arg)
        if input_path.is_file():
            # Make sure input_path is relative to input_dir
            print(
                f"Making sure input path {input_path} is inside the supplied project directory {project_dir}"
            )
            return InputParams(
                project_dir, RelPath(str(input_path.relative_to(project_dir)))
            )
        else:
            print(f"Assuming input path {input_arg} is relative to {project_dir}")
            return InputParams(project_dir, RelPath(input_arg))
    else:
        overall_input_path = pathlib.Path(input_arg)
        if overall_input_path.is_absolute():
            raise ValueError(
                f"Cannot infer project directory from absolute path '{input_arg}', please supply --project-dir argument"
            )
        assert (
            overall_input_path.is_file()
        ), f"Supplied turnip_text file '{input_arg}' isn't a file"

        if len(overall_input_path.parts) > 1:
            project_dir = pathlib.Path(overall_input_path.parts[0])
            print(f"Taking project directory as first path component: '{project_dir}'")
            input_rel_path = RelPath(*overall_input_path.parts[1:])
            return InputParams(project_dir, input_rel_path)
        else:
            print(f"Taking project directory as current working directory '.'")
            return InputParams(
                project_dir=pathlib.Path("."),
                input_rel_path=RelPath(str(overall_input_path)),
            )


@dataclass
class OutputParams:
    output_dir: pathlib.Path
    input_stem: str

def autodetect_output(output_arg: str, input_params: InputParams) -> OutputParams:
    """
    Given a --output argument determining the top-level folder containing subfolders for each document,
    and the input_params for a specific input document, determine the isolated output subfolder for the document {output}/{input.basename}/
    """
    output_base_dir = pathlib.Path(output_arg)
    if output_base_dir.exists() and not output_base_dir.is_dir():
        raise ValueError(
            f"Output base directory {output_base_dir} exists but isn't a directory. Please make it a folder."
        )
    elif not output_base_dir.exists():
        print(f"Output base directory {output_base_dir} does not exist, auto creating...")
        output_base_dir.mkdir(parents=False, exist_ok=True)
    
    input_name = input_params.input_rel_path.components[-1]
    input_stem = input_name.split(".", maxsplit=1)[0]
    if not input_stem:
        print(f"Trying to infer output folder from input file {input_name}, but it starts with a '.' and thus I cannot remove the extension.\nUsing the whole input file name as the output subdirectory.")
        output_subdir = input_name
    else:
        output_subdir = input_stem

    output_dir = output_base_dir / output_subdir
    print(f"Chose output directory {output_dir}")
    if output_dir.exists() and not output_dir.is_dir():
        raise ValueError(
            f"Output directory {output_dir} exists but isn't a directory. Please make it a folder."
        )
    elif not output_dir.exists():
        print(f"Output directory {output_dir} does not exist, auto creating...")
        output_dir.mkdir(parents=False, exist_ok=True)

    return OutputParams(output_dir, input_stem=output_subdir)

def render(
    input: InputParams,
    output: OutputParams,
    requested_format: str,
    setup: TurnipTextSetup,
    setup_kwargs: Dict[str, str],
) -> None:

    build_sys = BuildSystem(input.project_dir, output.output_dir)

    generated_setup = setup.generate_setup(
        input_stem=output.input_stem, requested_format=requested_format, **setup_kwargs
    )

    parse_and_emit(
        build_sys,
        src_path=input.input_rel_path,
        out_path=generated_setup.output_filename,
        render_setup=generated_setup.render_setup,
        plugins=generated_setup.plugins,
    )

    build_sys.close()
