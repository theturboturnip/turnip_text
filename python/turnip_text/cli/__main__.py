import argparse
import importlib
import inspect
import re
import textwrap
from typing import Any, Dict, List, Optional, Tuple, Type, cast

from turnip_text import (
    Block,
    BlockScopeBuilder,
    Header,
    Inline,
    InlineScopeBuilder,
    RawScopeBuilder,
)
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

# turnip_text files can override command-line arguments with shebang-esque comment lines at the start of the root file.
# The lines of the root file are parsed until they stop being comments, and of those file all that fit the `#tt-cli .*` pattern are checked.
#tt-cli setup=BLAH overrides the setup class the tool searches for.
#tt-cli setup-search-module=BLAH overrides the Python module turnip_text tries to import to *find* the setup class.
#tt-cli setup-arg=x:y sets the keyword argument x=y passed to the setup class when generating the plugins.
# All of these override the command-line arguments passed in.
TT_CLI_SHEBANG = re.compile(r"^#tt-cli\s+(.*)$")
SETUP_CLASS_SHEBANG = re.compile(r"setup=(\w+)")
SETUP_SEARCH_SHEBANG = re.compile(r"setup-search-module=(\w+)")
SETUP_KWARG_SHEBANG = re.compile(r"setup-arg=([^:]+:.*)")

def find_setup_class(
    input_params: InputParams, requested_setup_class: Optional[str], setup_args: List[str], setup_search_module_arg: str
) -> Tuple[TurnipTextSetup, Dict[str, str]]:
    setup_kwargs = parse_setup_kwargs(setup_args)

    # Peek at the first lines of the file to override the requested_setup_class and setup_kwargs
    shebang_lines = []
    with open(
        input_params.project_dir / str(input_params.input_rel_path),
        "r",
        encoding="utf-8",
    ) as f:
        while True:
            line = f.readline()
            if not line.startswith("#"):
                break
            match = TT_CLI_SHEBANG.match(line)
            if match:
                shebang_lines.append(match.group(1))

    # Get list of all shebang lines that match the SETUP_CLASS regex
    setup_class_shebang_lines: List[re.Match] = list(filter(None, (SETUP_CLASS_SHEBANG.match(line) for line in shebang_lines)))
    # Get list of all shebang lines that match the SETUP_SEARCH regex
    setup_class_search_shebang_lines: List[re.Match] = list(filter(None, (SETUP_SEARCH_SHEBANG.match(line) for line in shebang_lines)))
    # Get list of all shebang lines that match the SETUP_ARG regex
    setup_kwarg_shebang_lines: List[re.Match] = list(filter(None, (SETUP_KWARG_SHEBANG.match(line) for line in shebang_lines)))

    # Update the setup_class based on the shebang
    if setup_class_shebang_lines:
        if len(setup_class_shebang_lines) > 1:
            raise RuntimeError(f"Can't use the `#tt-cli setup=` shebang multiple times: found {setup_class_shebang_lines} in file {input_params.input_rel_path}")
        requested_setup_class = setup_class_shebang_lines[0].group(1)
        print(
            f"Taking requested setup class from input file shebang: '{requested_setup_class}'"
        )
    elif requested_setup_class:
        print(
            f"Taking requested setup class from command line: '{requested_setup_class}'"
        )

    # Update the setup class search module
    if setup_class_search_shebang_lines:
        if len(setup_class_search_shebang_lines) > 1:
            raise RuntimeError(f"Can't use the `#tt-cli setup-class-search=` shebang multiple times: found {setup_class_search_shebang_lines} in file {input_params.input_rel_path}")
        setup_search_module_arg = setup_class_search_shebang_lines[0].group(1)
        print(
            f"Taking requested setup class search module from input file shebang: '{setup_search_module_arg}'"
        )

    # Update the setup_kwargs
    setup_kwargs.update(parse_setup_kwargs([setup_kwarg.group(1) for setup_kwarg in setup_kwarg_shebang_lines]))

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
        return setup_class(), setup_kwargs
    else:
        return DefaultTurnipTextSetup(), setup_kwargs


def parse_setup_kwargs(setup_args: List[str]) -> Dict[str, str]:
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

            setup, setup_kwargs = find_setup_class(input_params, args.setup, args.setup_args, args.setup_search_module)

            render(
                input_params,
                output_params,
                format,
                setup,
                setup_kwargs,
            )


def wrap_describe(args: Any) -> None:
    input_params = autodetect_input(args.inputs[0], args.project_dir)
    setup, setup_kwargs = find_setup_class(input_params, args.setup, args.setup_args, args.setup_search_module)

    print(f"This document uses the {setup.__class__.__qualname__} class as its Setup.")
    if setup_kwargs:
        print(f"The following keyword arguments are passed in:\n")
        for key, value in setup_kwargs.items():
            print("\t", key, ":\t", value)
    else:
        print(f"No kwargs have been passed in.")

    if args.plugins and setup.__doc__:
        print("-" * 20)
        print(f"class {setup.__class__.__name__}:")
        print("\t" + textwrap.dedent(setup.__doc__).replace("\n", "\n\t"))
    else:
        help(setup)

    if args.plugins:
        plugins = setup.generate_setup(**setup.DEFAULT_INPUTS, **setup_kwargs).plugins
        print(
            f"The setup class generates a list of approximately {len(plugins)} plugins, with the following interfaces"
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

            if not (inspect.isfunction(item) or inspect.ismethod(item)):
                fits_block = isinstance(item, Block)
                fits_inline = isinstance(item, Inline)
                fits_header = isinstance(item, Header)
                is_doc_element = fits_block or fits_inline or fits_header
                if inspect.isclass(item):
                    fits_bsb = issubclass(item, BlockScopeBuilder)
                    fits_isb = issubclass(item, InlineScopeBuilder)
                    fits_rsb = issubclass(item, RawScopeBuilder)
                else:
                    fits_bsb = isinstance(item, BlockScopeBuilder)
                    fits_isb = isinstance(item, InlineScopeBuilder)
                    fits_rsb = isinstance(item, RawScopeBuilder)
                is_doc_builder = fits_bsb or fits_isb or fits_rsb

                if is_doc_element or is_doc_builder:
                    if inspect.isclass(item):
                        print(
                            f"class {name}({', '.join(str(base) for base in item.__bases__ if base is not object)})"
                        )
                    else:
                        things_it_fits = []
                        if fits_block:
                            things_it_fits.append("Block")
                        if fits_inline:
                            things_it_fits.append("Inline")
                        if fits_header:
                            things_it_fits.append("Header")

                        if fits_bsb:
                            things_it_fits.append("BlockScopeBuilder")
                        if fits_isb:
                            things_it_fits.append("InlineScopeBuilder")
                        if fits_rsb:
                            things_it_fits.append("RawScopeBuilder")

                        if is_doc_builder:
                            print(f"{name} = ({', '.join(things_it_fits)})")
                        else:
                            print(
                                f"{name} = {item!r} (fits {', '.join(things_it_fits)})"
                            )
                    if item.__doc__:
                        print(
                            "\t" + textwrap.dedent(item.__doc__).replace("\n", "\n\t")
                        )
                    else:
                        print("\tNo documentation found.")
                else:
                    print(f"{name} = {item!r}")
            # The item may also be callable.
            if callable(item) and not inspect.isclass(item):
                signature = inspect.signature(getattr(doc_env, name))
                if not signature.parameters:
                    print(f"def {name}{signature}:")
                else:
                    print(f"def {name}(")
                    for param in signature.parameters.values():
                        print(f"\t{param},")
                    print(f") -> {signature.return_annotation}:")
                if inspect.isfunction(item) or inspect.ismethod(item):
                    if item.__doc__:
                        print(
                            "\t" + textwrap.dedent(item.__doc__).replace("\n", "\n\t")
                        )
                    else:
                        print("\tNo documentation found.")
                else:
                    if item.__call__.__doc__:
                        print(
                            "\t"
                            + textwrap.dedent(item.__call__.__doc__).replace(
                                "\n", "\n\t"
                            )
                        )
                    else:
                        print("\tNo documentation found.")
            # TODO if it supports a builder, give a usage example?


def run_cli() -> None:
    parser = argparse.ArgumentParser("turnip_text.cli")

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
        required=True,
        nargs="+",
        help="The format(s) to export to for each input file. Can be any value accepted by the setup, common accepted values are 'latex', 'html', 'markdown', 'pandoc-{format}'.",
    )
    render_subcommand.add_argument(
        "--project-dir",
        type=str,
        default=None,
        help="The 'project' directory, where all accessible input files are stored.",
    )
    render_subcommand.add_argument(
        "--setup",
        type=str,
        default=None,
        help="The name of the TurnipTextSetup class to use, which generates the plugins and execution environment for the renderer.\nMust *directly* inherit from TurnipTextSetup. Can be inferred from a '#tt-cli setup:OtherSetupClassHere' shebang line in the input file.",
    )
    render_subcommand.add_argument(
        "--setup-search-module",
        default="custom_turnip_text",
        help="The name of the Python module that turnip_text loads to search for a `--setup` it doesn't know about yet.\nSet to the empty string '' to disable this behaviour.",
    )
    render_subcommand.add_argument(
        "--setup-args",
        nargs="*",
        type=str,
        help="Colon-separated arguments to the setup, e.g. 'csl_bib:bibliography_csl.json'. Use 'turnip_text.cli describe' to see the setup documentation and what arguments it uses.",
    )
    # If the render subcommand is selected, set `args.func = wrap_render`
    render_subcommand.set_defaults(func=wrap_render)

    describe_subcommand = subparsers.add_parser(
        "show-setup", help="Describe the TurnipTextSetup used for this configuration."
    )
    describe_subcommand.add_argument(
        "inputs",
        type=str,
        nargs="+",
        help="The input turnip_text files (usually with a .ttext extension). If `--project-dir` is not set, this is used to infer the 'project' directory where all other input files live.",
    )
    describe_subcommand.add_argument(
        "--plugins",
        action="store_true",
        help="Describe the plugin interface for this configuration, not just the TurnipTextSetup. This will be large, recommend piping this into `less`.",
    )
    describe_subcommand.add_argument(
        "--project-dir",
        type=str,
        default=None,
        help="The 'project' directory, where all accessible input files are stored.",
    )
    describe_subcommand.add_argument(
        "--setup",
        type=str,
        default=None,
        help="The name of the TurnipTextSetup class to use, which generates the plugins and execution environment for the renderer.\nMust *directly* inherit from TurnipTextSetup. Can be inferred from a '#tt-cli setup:OtherSetupClassHere' shebang line in the input file.",
    )
    describe_subcommand.add_argument(
        "--setup-search-module",
        default="custom_turnip_text",
        help="The name of the Python module that turnip_text loads to search for a `--setup` it doesn't know about yet.\nSet to the empty string '' to disable this behaviour.",
    )
    describe_subcommand.add_argument(
        "--setup-args",
        nargs="*",
        type=str,
        help="Colon-separated arguments to the setup, e.g. 'csl_bib:bibliography_csl.json'. Use 'turnip_text.cli describe' to see the setup documentation and what arguments it uses.",
    )

    describe_subcommand.set_defaults(func=wrap_describe)

    args = parser.parse_args()
    # call args.func() with the args, should be wrap_render
    args.func(args)


if __name__ == "__main__":
    run_cli()
