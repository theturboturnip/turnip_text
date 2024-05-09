from dataclasses import dataclass
from typing import Dict, Iterable, List, Set


@dataclass
class LatexPackageRequirements:
    package: str
    reasons: List[str]
    options: Set[str]

    def as_latex_preamble_comment(self) -> str:
        line = f"\\usepackage"
        if self.options:
            line += f"[{','.join(self.options)}]"
        line += f"{{{self.package}}}"
        if self.reasons:
            line += f" % for {', '.join(self.reasons)}"
        return line


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
        self, package: str, reason: str, options: str | Iterable[str] | None = None
    ) -> None:
        package_obj = self.requested_packages.get(package, None)
        if isinstance(options, str):
            options_set = {options}
        elif options is None:
            options_set = set()
        else:
            # options is some iterable
            options_set = set(options)
        if package_obj is None:
            # Wasn't in the dict
            self.requested_packages[package] = LatexPackageRequirements(
                package=package,
                reasons=[reason],
                options=options_set,
            )
        else:
            package_obj.reasons.append(reason)
            # TODO check conflicts between options
            package_obj.options.update(options_set)
