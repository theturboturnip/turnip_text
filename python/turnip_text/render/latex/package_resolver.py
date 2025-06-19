import graphlib
from dataclasses import dataclass
from typing import Dict, List, Optional, Sequence, Set, Tuple, Union
import warnings

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
    already_included_by_document: bool

    def as_latex_preamble_line(self, with_reason: bool, macro="usepackage") -> str:
        line = f"\\{macro}"
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
    """A well-formed, correctly ordered, list of LaTeX packages to import with valid options."""


# FUTURE: Try to make reasons for packages dependent on their actual use, not just "this plugin could theoretically use this package"
class LatexPackageResolver:
    shell_escape_reasons: List[str]
    """Reasons plugins called .request_shell_escape()"""
    requested_packages: Dict[str, LatexPackageRequirements]
    """Requested packages built up through calls to .request_latex_package()"""
    whitelisted_packages: Set[str]
    """Packages that are whitelisted by the document class - if not empty, all packages used must be in this set"""

    def __init__(self) -> None:
        self.shell_escape_reasons = []
        self.requested_packages = {}
        self.whitelisted_packages = set()

    def register_package_whitelist(self, *whitelist: str) -> None:
        self.whitelisted_packages.update(whitelist)

    def register_class_preexisting_packages(self, *preexisting: str) -> None:
        # TODO add package options to this, if user tries to set a conflicting option we need to complain, if user doesn't set any new options don't need to include it in the render
        for package in preexisting:
            self.request_latex_package(package, "docclass", used_by_docclass=True)

    def request_shell_escape(self, reason: str) -> None:
        self.shell_escape_reasons.append(reason)

    def request_latex_package(
        self, package: str, reason: str, options: Sequence[LatexPackageOption] = [], used_by_docclass: bool = False,
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
                already_included_by_document=used_by_docclass
            )
            self.requested_packages[package] = package_obj

        package_obj.reasons.append(reason)
        package_obj.options.extend(options)
        if used_by_docclass:
            package_obj.already_included_by_document = used_by_docclass

    def resolve_all(self) -> ResolvedLatexPackages:
        # Step 1: resolve all the package options
        # (I'm pretty sure theoretically options may affect ordering, but I'm not 100% on that.)
        all_packages = set(self.requested_packages.keys())
        if self.whitelisted_packages:
            requested_not_whitelisted = all_packages.difference(self.whitelisted_packages)
            infos = []
            for package_name in requested_not_whitelisted:
                package = self.requested_packages[package_name]
                if package.already_included_by_document:
                    continue
                infos.append(f"Package '{package_name}' requested because {', '.join(package.reasons)}")
            if infos:
                msg = f"Requested packages that were not in the whitelist:\n" + "\n".join(infos)
                # raise RuntimeError(msg)
                # TODO make this a hard error?
                warnings.warn(msg, RuntimeWarning)
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
        case "hyperref":
            deduped_opts = remove_dupe_options(package, opts)
            # Section 11 of hyperref documentation
            if "footnote" in all_packages:
                deduped_opts["hyperfootnotes"] = "false"
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
        # FUTURE it may be nice to treat this as a "soft" requirement?
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
            # TODO this is benign - need an incompat warning, not an incompat error
            # "titlesec",
            "ucs",
            "etextools",
        }
        incompat.intersection_update(set(packages.keys()))
        if incompat:
            raise_compat_error(
                "biblatex", *incompat, reason="BibLaTeX documentation says so"
            )

    # From hyperref docs:
    # "Package longtable must be put before hyperref and arydshln,
    # hyperref after arydshln generates an error"
    sorter.add("hyperref", "longtable")
    sorter.add("arydshln", "longtable")
    sorter.add("arydshln", "hyperref")

    # Other from hyperref docs
    sorter.add("titleref", "nameref")
    sorter.add("hyperref", "titleref")

    # Technically ltabptch not necessary anymore...
    if "longtable" in packages:
        sorter.add("ltabptch", "longtable")

    if "hyperref" in packages:
        # Section 11 of hyperref documentation

        sorter.add("algorithm", "hyperref")
        sorter.add(
            "amsrefs", "hyperref"
        )  # hyperref docs note it is unclear if this is necessary
        sorter.add(
            "chappg", "hyperref"
        )  # chappg uses something that hyperref redefines
        sorter.add("dblaccnt", "hyperref")
        sorter.add("ellipsis", "hyperref")
        sorter.add("hyperref", "float")
        sorter.add("linguex", "hyperref")
        sorter.add("hyperref", "longtable")
        sorter.add("hyperref", "ltabptch")
        sorter.add("hyperref", "multind")
        sorter.add("hyperref", "natbib")
        sorter.add("hyperref", "setspace")
        sorter.add("hyperref", "vietnam")

        # TODO how to handle tabularx
        # "Linked footnotes are not supported inside environment tabularx, because they uses the optional argument of \footnotetext, see section ‘Limitations’.

        incompat = {
            "bibentry",  # Technically this requires a workaround
            "bigfoot",
            "count1to",  # Technically this requires a workaround
            "easyeqn",
            "endnotes",
            # foiltex < 2.1.4b fails, ignoring because people must have upgraded by now
            "mathenv",  # Breaks eqnarray
            "minitoc-hyper",
            # "nomencl", # it's substandard because it doesn't link to the nomenclature
            "ntheorem",
            "ntheorem-hyper",
            "prettyref",  # Requires a workaround
            # "titlesec", # TODO Requires a workaround (https://tex.stackexchange.com/questions/397031/conflict-with-hyperref-and-titlesec)
        }
        incompat.intersection_update(set(packages.keys()))
        if incompat:
            raise_compat_error(
                "hyperref", *incompat, reason="hyperref documentation says so"
            )

    return [
        packages[package_name]
        for package_name in sorter.static_order()
        if package_name in packages
    ]
