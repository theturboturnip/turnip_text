import pathlib
from dataclasses import dataclass
from enum import IntEnum
from typing import Any, Dict, List, Optional, Tuple, Union

from turnip_text.build_system import (
    BuildSystem,
    RelPath,
    RelPathComponent,
    check_component,
)
from turnip_text.render import RenderPlugin, RenderSetup
from turnip_text.system import parse_and_emit


class TurnipTextSuggestedRenderer(IntEnum):
    Latex = 0
    Markdown = 1
    HTML = 2
    Pandoc = 3


def suggest_renderer(output_name: str) -> TurnipTextSuggestedRenderer:
    if output_name.endswith(".tex"):
        return TurnipTextSuggestedRenderer.Latex
    if output_name.endswith(".md"):
        return TurnipTextSuggestedRenderer.Markdown
    if output_name.endswith(".html") or output_name.endswith(".htm"):
        return TurnipTextSuggestedRenderer.HTML
    return TurnipTextSuggestedRenderer.Pandoc


class TurnipTextSetup:
    """
    A class which generates RenderSetups with a consistent plugin interface
    for different renderers.

    turnip_text files can request a TurnipTextSetup (indicating what plugins they use) through a shebang,
    which the turnip_text CLI will autodetect when trying to render the file.
    """

    DEFAULT_RENDERER: "TurnipTextSuggestedRenderer" = TurnipTextSuggestedRenderer.Latex
    """
    A value that when passed to generate_setup() will always result in a valid RenderSetup and plugins.
    
    Used by the turnip_text CLI to provide documentation on available plugin functions.
    """

    def generate_setup(
        self,
        suggestion: "TurnipTextSuggestedRenderer",
        **kwargs: str,
        # Unfortunately we can't express the constraint ("whatever tuple is returned must have matching TRenderSetup")
        # ) -> Tuple[TRenderSetup, List[RenderPlugin[TRenderSetup]]]:
    ) -> Tuple[RenderSetup, List[RenderPlugin[Any]]]:
        """
        Given
        - a turnip_text suggested renderer code
        - a user-defined string

        return a RenderSetup and a list of plugins to use with that RenderSetup to render a turnip_text file.
        Otherwise, raise NotImplementedError.
        """
        raise NotImplementedError(
            f"{self.__class__.__name__} doesn't have a setup for suggested renderer '{suggestion}'"
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
    output_rel_path: RelPathComponent  # Must be a direct descendant of the path


def autodetect_output(output_arg: str) -> OutputParams:
    output_path = pathlib.Path(output_arg)
    output_dir = output_path.parent
    if output_dir.exists() and not output_dir.is_dir():
        raise ValueError(
            f"Output directory {output_dir} exists but isn't a directory. Please make it a folder."
        )
    elif not output_dir.exists():
        print(f"Output directory {output_dir} does not exist, auto creating...")
        output_dir.mkdir(parents=False, exist_ok=True)
    output_rel_path = output_path.relative_to(output_dir)
    if len(output_rel_path.parts) != 1:
        raise ValueError(
            f"Somehow output file {output_arg} is not a direct child of its parent {output_dir}."
        )
    return OutputParams(
        output_dir, output_rel_path=check_component(output_rel_path.parts[0])
    )


def render(
    input: InputParams,
    output: OutputParams,
    setup: TurnipTextSetup,
    setup_kwargs: Dict[str, str],
) -> None:

    build_sys = BuildSystem(input.project_dir, output.output_dir)

    suggestion = suggest_renderer(output.output_rel_path)

    render_setup, plugins = setup.generate_setup(
        suggestion, **setup_kwargs
    )  # type:ignore

    parse_and_emit(
        build_sys,
        src_path=input.input_rel_path,
        out_path=output.output_rel_path,
        render_setup=render_setup,
        plugins=plugins,
    )

    build_sys.close()
