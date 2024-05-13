import argparse
import importlib
import inspect
import re
import textwrap
from typing import Any, Dict, List, Optional, Type, cast

from turnip_text.cli import (
    InputParams,
    TurnipTextSetup,
    autodetect_input,
    autodetect_output,
    render,
)
from turnip_text.cli.default_setup import DefaultTurnipTextSetup
from turnip_text.env_plugins import EnvPlugin
from turnip_text.plugins.anchors import StdAnchorPlugin

# TODO enable indirect inheritance of TurnipTextSetup?

SETUP_CLASS_SHEBANG = re.compile(r"#tt-cli\s+setup:(\w+)")


def find_setup_class(
    input_params: InputParams, setup_arg: Optional[str], setup_search_module_arg: str
) -> TurnipTextSetup:
    requested_setup_class: Optional[str] = None
    # Peek at the file to see if there's a shebang
    # TODO file encoding
    with open(
        input_params.project_dir / str(input_params.input_rel_path),
        "r",
        encoding="utf-8",
    ) as f:
        match = SETUP_CLASS_SHEBANG.match(f.readline())
    if match:
        requested_setup_class = match.group(1)
        print(
            f"Taking requested setup class from input file shebang: '{requested_setup_class}'"
        )
    elif setup_arg:
        requested_setup_class = setup_arg
        print(
            f"Taking requested setup class from command line: '{requested_setup_class}'"
        )

    if requested_setup_class:
        # Look for subclasses of TurnipTextSetup that match the requested name
        setup_classes = [
            cls
            for cls in TurnipTextSetup.__subclasses__()
            if cls.__name__ == requested_setup_class
        ]
        if len(setup_classes) == 0:
            if setup_search_module_arg:
                print(
                    f"Searching for setup class '{requested_setup_class}' in setup-search-module '{setup_search_module_arg}'"
                )
                if setup_search_module_arg.startswith("."):
                    raise ValueError(
                        f"--setup-search-module cannot be a relative import"
                    )
                try:
                    # Import the requested module, which will hopefully create the setup, then look again
                    importlib.import_module(setup_search_module_arg)
                    setup_classes = [
                        cls
                        for cls in TurnipTextSetup.__subclasses__()
                        if cls.__name__ == requested_setup_class
                    ]
                except ImportError:
                    raise RuntimeError(f"Failed to import the setup-search-module")

                if not setup_classes:
                    raise RuntimeError(
                        f"Couldn't find setup class '{requested_setup_class}', even in --setup-search-module.\nMake sure it's in that module and subclasses TurnipTextSetup."
                    )
            else:
                raise RuntimeError(
                    f"Couldn't find setup class '{requested_setup_class}', and --setup-search-module was disabled."
                )

        if len(setup_classes) > 1:
            print(
                f"Found multiple candidate setup classes for '{requested_setup_class}' [{setup_classes}]. Picking the first one."
            )
        # If we get an abstract type here we'll pick that up when we construct it.
        setup_class: Type[TurnipTextSetup] = setup_classes[0]
        print(f"Creating a setup class {setup_class} with zero arguments")
        return setup_class()
    else:
        return DefaultTurnipTextSetup()


def parse_setup_kwargs(setup_args: Optional[str]) -> Dict[str, str]:
    setup_kwargs: Dict[str, str] = {}
    if setup_args:
        for setup_arg in setup_args:
            key, value = setup_arg.split(":", maxsplit=1)
            setup_kwargs[key] = value
    return setup_kwargs


def wrap_render(args: Any) -> None:
    setup_kwargs = parse_setup_kwargs(args.setup_args)
    for input_arg in args.inputs:
        for format in args.formats:
            input_params = autodetect_input(input_arg, args.project_dir)

            output_params = autodetect_output(args.output_dir, input_params)

            setup = find_setup_class(input_params, args.setup, args.setup_search_module)

            render(
                input_params,
                output_params,
                format,
                setup,
                setup_kwargs,
            )


def wrap_describe(args: Any) -> None:
    input_params = autodetect_input(args.inputs[0], args.project_dir)
    setup = find_setup_class(input_params, args.setup, args.setup_search_module)
    setup_kwargs = parse_setup_kwargs(args.setup_args)

    help(setup)

    print(f"This document uses the {setup.__class__.__qualname__} class as its Setup.")
    print(f"The following keyword arguments are passed in:\n")
    for key, value in setup_kwargs.items():
        print("\t", key, ":\t", value)

    if args.plugins:
        plugins = setup.generate_setup(**setup.DEFAULT_INPUTS, **setup_kwargs).plugins
        print(
            f"The setup class generates a list of approximately {len(plugins)} plugins."
        )

        anchors = StdAnchorPlugin()
        plugins_with_anchors: List[EnvPlugin] = list(plugins)
        plugins_with_anchors.append(anchors)
        fmt, doc_env = EnvPlugin._make_contexts(
            # Don't need the build system here
            None,  # type:ignore
            plugins_with_anchors,
        )

        # TODO fmt vs doc_env
        # TODO docstrings could be way better here... especially for builders
        for name, item in doc_env.__dict__.items():
            print("-" * 20)
            if inspect.isfunction(item) or inspect.ismethod(item):
                print(f"def {name}{inspect.signature(getattr(doc_env, name))}:")
            else:
                print(f"{name} = {item}")
            if item.__doc__:
                print("\t" + textwrap.dedent(item.__doc__).replace("\n", "\n\t"))
            else:
                print("\tNo documentation found.")
            # TODO if it supports a builder, give a usage example?


def run_cli() -> None:
    parser = argparse.ArgumentParser("turnip_text.cli")

    parser.add_argument(
        "--project-dir",
        type=str,
        default=None,
        help="The 'project' directory, where all accessible input files are stored.",
    )
    parser.add_argument(
        "--setup",
        type=str,
        default=None,
        help="The name of the TurnipTextSetup class to use, which generates the plugins and execution environment for the renderer.\nMust *directly* inherit from TurnipTextSetup. Can be inferred from a '#tt-cli setup:OtherSetupClassHere' shebang line in the input file.",
    )
    parser.add_argument(
        "--setup-search-module",
        default="custom_turnip_text",
        help="The name of the Python module that turnip_text loads to search for a `--setup` it doesn't know about yet.\nSet to the empty string '' to disable this behaviour.",
    )
    parser.add_argument(
        "--setup-args",
        nargs="*",
        type=str,
        help="Colon-separated arguments to the setup, e.g. 'csl_bib:bibliography_csl.json'. Use 'turnip_text.cli describe' to see the setup documentation and what arguments it uses.",
    )

    subparsers = parser.add_subparsers(required=True)

    render_subcommand = subparsers.add_parser(
        "render", help="Render a turnip_text document out into a document file."
    )
    render_subcommand.add_argument(
        "inputs",
        type=str,
        nargs="+",
        help="The input turnip_text files (usually with a .ttext extension). If `--project-dir` is not set, each is used to separately infer the 'project' directory where all other input files live.",
    )
    render_subcommand.add_argument(
        "-o",
        "--output-dir",
        type=str,
        required=True,
        help="The toplevel folder for document outputs. Generates output folders {output}/{input_basename}/<output files for each input>",
    )
    render_subcommand.add_argument(
        "--formats",
        type=str,
        nargs="+",
        help="The format(s) to export to for each input file. Can be any value accepted by the setup, common accepted values are 'latex', 'html', 'markdown', 'pandoc-{format}'.",
    )
    # If the render subcommand is selected, set `args.func = wrap_render`
    render_subcommand.set_defaults(func=wrap_render)

    describe_subcommand = subparsers.add_parser(
        "show-setup", help="Describe the TurnipTextSetup used for this configuration."
    )
    describe_subcommand.add_argument(
        "input",
        type=str,
        # nargs="+", # TODO
        help="The input turnip_text file (usually with a .ttext extension). If `--project-dir` is not set, this is used to infer the 'project' directory where all other input files live.",
    )
    describe_subcommand.add_argument(
        "--plugins",
        action="store_true",
        help="Describe the plugin interface for this configuration, not just the TurnipTextSetup. This will be large, recommend piping this into `less`.",
    )

    describe_subcommand.set_defaults(func=wrap_describe)

    args = parser.parse_args()
    # call args.func() with the args, should be wrap_render
    args.func(args)


if __name__ == "__main__":
    run_cli()
