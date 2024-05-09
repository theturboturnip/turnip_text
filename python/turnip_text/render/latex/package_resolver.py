import graphlib
from dataclasses import dataclass
from typing import Dict, List, Optional, Sequence, Set, Tuple, Union

LatexPackageOptions = List["LatexPackageOption"]
"""turnip_text treats the options to packages as a comma-separated order-sensitive list of options."""

LatexDedupedPackageOptions = Dict[str, Union[str, None]]
"""An alternative deduplicated representation of options, still ordered.
If the value for a key is None that option is a plain string."""

LatexPackageOption = Union[str, Tuple[str, str]]
"""turnip_text believes there are two kinds of option: a plain string, and key-value."""


@dataclass
class LatexPackageRequirements:
    package: str
    reasons: List[str]
    options: LatexPackageOptions

    def as_latex_preamble_line(self, with_reason: bool) -> str:
        line = f"\\usepackage"
        if self.options:
            opt_strs = [
                opt if isinstance(opt, str) else f"{opt[0]}={opt[1]}"
                for opt in self.options
            ]
            line += f"[{','.join(opt_strs)}]"
        line += f"{{{self.package}}}"
        if with_reason and self.reasons:
            line += f" % for {', '.join(self.reasons)}"
        return line


@dataclass
class ResolvedLatexPackages:
    shell_escape_reasons: List[str]
    """Reasons the --shell-escape command-line flag is necessary"""
    packages: List[LatexPackageRequirements]
    """A well-formed, (TODO correctly ordered), list of LaTeX packages to import with valid options."""


# FUTURE: Try to make reasons for packages dependent on their actual use, not just "this plugin could theoretically use this package"
class LatexPackageResolver:
    shell_escape_reasons: List[str]
    """Reasons plugins called .request_shell_escape()"""
    requested_packages: Dict[str, LatexPackageRequirements]
    """Requested packages built up through calls to .request_latex_package()"""

    # TODO resolve_all() method to fixup package order, package options

    def __init__(self) -> None:
        self.shell_escape_reasons = []
        self.requested_packages = {}

    def request_shell_escape(self, reason: str) -> None:
        self.shell_escape_reasons.append(reason)

    def request_latex_package(
        self, package: str, reason: str, options: Sequence[LatexPackageOption] = []
    ) -> None:
        # str is a Sequence[str] returning each character in the str
        # Very low chance of anyone calling this function expecting
        # that it treat each character as a separate option.
        # Instead, replace options with a list containing that string.
        if isinstance(options, str):
            options = [options]
        package_obj = self.requested_packages.get(package, None)
        if package_obj is None:
            # Wasn't in the dict
            package_obj = LatexPackageRequirements(
                package=package,
                reasons=[],
                options=[],
            )
            self.requested_packages[package] = package_obj

        package_obj.reasons.append(reason)
        package_obj.options.extend(options)

    def resolve_all(self) -> ResolvedLatexPackages:
        # Step 1: resolve all the package options
        # (I'm pretty sure theoretically options may affect ordering, but I'm not 100% on that.)
        all_packages = set(self.requested_packages.keys())
        for package in self.requested_packages.values():
            # Pass in the list of all packages - this is because it might be nice to add options to packages
            # if they need to be compatible with other packages
            package.options = resolve_package_options(
                package.package, package.options, all_packages
            )
        # Step 2: order the packages
        packages = order_packages(self.requested_packages)
        return ResolvedLatexPackages(
            shell_escape_reasons=self.shell_escape_reasons, packages=packages
        )


def remove_dupe_options(
    package: str, opts: LatexPackageOptions
) -> LatexDedupedPackageOptions:
    """
    Remove duplicate options.
    "duplicate" means the key is the same if the option is key-value, or the string is the same if it's just a string.
    Raises ValueError on conflicting duplicates
    i.e. if a key-value option has a key that matches a non-key-value option,
    or if two key-value options have the same key but different values.
    """

    deduped_opts: LatexDedupedPackageOptions = dict()
    for o in opts:
        key: str
        val: Union[str, None]
        if isinstance(o, str):
            key = o
            val = None
        else:
            key, val = o

        if key in deduped_opts:
            previous_val = deduped_opts[key]
            if val != previous_val:
                raise ValueError(
                    f"Conflicting duplicate option for package '{package}': '{key}' defined as {repr(previous_val)} and {repr(val)}"
                )
        deduped_opts[key] = val
    return deduped_opts


def force_ordering(
    opts: LatexDedupedPackageOptions, order: List[str]
) -> LatexDedupedPackageOptions:
    """TODO: This could be a useful convenience method.

    Given `order`, the ordering of all(?) possible option keys,
    return an options dict with all keys in that order, throwing ValueError if an option is not present in the ordering.
    If a key is present in the ordering but not in the options, that's fine."""
    raise NotImplementedError()


def as_list(opts: LatexDedupedPackageOptions) -> LatexPackageOptions:
    return [k if v is None else (k, v) for k, v in opts.items()]


def resolve_package_options(
    package: str, opts: LatexPackageOptions, all_packages: Set[str]
) -> LatexPackageOptions:
    # Optionally provide special handling for specific packages
    match package:
        # An example of special-case handling:
        # Automatically fixup a conflict between ulem and apacite.
        # https://tex.stackexchange.com/a/659241
        case "ulem":
            deduped_opts = remove_dupe_options(package, opts)
            if "apacite" in all_packages and "normalem" not in deduped_opts:
                deduped_opts["normalem"] = None
            return as_list(deduped_opts)
        case _:
            deduped_opts = remove_dupe_options(package, opts)
            return as_list(deduped_opts)


def order_packages(
    packages: Dict[str, LatexPackageRequirements]
) -> List[LatexPackageRequirements]:
    """Find a correct order of packages based on what their options are.

    Raises ValueError if some packages are not compatible with others.
    """

    def raise_compat_error(*package_names: str, reason: Optional[str] = None) -> None:
        raise ValueError(
            f"Package {package_names[0]} not compatible with {[', '.join(package_names[1:])]}. Reason: {reason}"
            + "".join(
                (
                    "\n"
                    + f"Included {package} because {', '.join(packages[package].reasons)}"
                )
                for package in package_names
            )
        )

    # Use graphlib to build a DAG of packages
    sorter = graphlib.TopologicalSorter({package: [] for package in packages})

    # As per cleveref documentation, the correct order is varioref, hyperref, cleveref
    sorter.add("hyperref", "varioref")
    sorter.add("cleveref", "hyperref")

    if "cleveref" in packages:
        # https://mirror.its.dal.ca/ctan/macros/latex/contrib/cleveref/cleveref.pdf
        # hypdvips and autonum should be loaded afterwards
        sorter.add("hypdvips", "cleveref")
        sorter.add("autonum", "cleveref")
        # cleveref should be loaded effectively after everything else
        sorter.add(
            "cleveref",
            *[
                package
                for package in packages.keys()
                if package not in ["hypdvips", "autonum", "cleveref"]
            ],
        )
        if "mathtools" in packages and "showonlyrefs" in packages["mathtools"].options:
            raise_compat_error(
                "cleveref",
                "mathtools",
                reason="Cleveref is incompatible with the showonlyrefs option of the mathtools package",
            )

    if "biblatex" in packages:
        # https://mirrors.ibiblio.org/CTAN/macros/latex/contrib/biblatex/doc/biblatex.pdf
        # "When using the hyperref package, it is preferable to load it after biblatex"
        sorter.add("hyperref", "biblatex")

        # Section 1.5.5 incompatible packages
        incompat = {
            "babelbib",
            "backref",
            "bibtopic",
            "bibunits",
            "chapterbib",
            "cite",
            "citeref",
            "inlinebib",
            "jurabib",
            "mcite",
            "mciteplus",
            "multibib",
            "natbib",
            "splitbib",
            "titlesec",
            "ucs",
            "etextools",
        }
        incompat.intersection_update(*packages.keys())
        if incompat:
            raise_compat_error(
                "biblatex", *incompat, reason="BibLaTeX documentation says so"
            )

    return [
        packages[package_name]
        for package_name in sorter.static_order()
        if package_name in packages
    ]
