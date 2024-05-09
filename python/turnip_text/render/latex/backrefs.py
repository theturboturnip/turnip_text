from enum import IntEnum
from typing import Dict

from typing_extensions import override

from turnip_text import Raw, Text
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.env_plugins import FmtEnv
from turnip_text.render.latex.package_resolver import LatexPackageResolver
from turnip_text.render.latex.renderer import (
    LatexBackrefMethodImpl,
    LatexCounterFormat,
    LatexCounterSpec,
    LatexRenderer,
)


class LatexBackrefMethod(IntEnum):
    Cleveref = 0
    Hyperlink = 1
    PageRef = 2


# TODO latex \autoref support?
# TODO proper capitalization at the start of sentences


class LatexHyperlink(LatexBackrefMethodImpl):
    """A means of referring back to a position in the document using the \\hypertarget and \\hyperlink macros instead of \\label and \\XXXref"""

    manual_counter_method: Dict[str, LatexCounterFormat]

    description: str = "\\hypertarget and \\hyperlink"

    def __init__(self) -> None:
        super().__init__()
        self.manual_counter_method = {}

    @override
    def request_packages(self, package_resolver: LatexPackageResolver) -> None:
        package_resolver.request_latex_package("hyperref", "backrefs")

    @override
    def emit_config_for_fmt(
        self, spec: LatexCounterSpec, _renderer: LatexRenderer
    ) -> None:
        self.manual_counter_method[spec.latex_counter] = spec.get_manual_fmt()

    @override
    def emit_anchor(
        self, anchor: Anchor, renderer: "LatexRenderer", fmt: FmtEnv
    ) -> None:
        renderer.emit_raw(
            f"\\hypertarget{{{anchor.canonical()}}}{{}}"
        )  # TODO include caption for anchor?

    @override
    def emit_backref(
        self,
        backref: Backref,
        anchor: Anchor,
        renderer: "LatexRenderer",
        _fmt: FmtEnv,
    ) -> None:
        renderer.emit_macro("hyperlink")
        renderer.emit_braced(Raw(anchor.canonical()))
        if backref.label_contents is None:
            renderer.emit_braced(renderer.get_resolved_anchor_text(anchor))
        else:
            renderer.emit_braced(backref.label_contents)


class LatexCleveref(LatexBackrefMethodImpl):
    """Cross-referencing using the `cleveref` LaTeX package, which has built-in sane defaults for many counters and languages. If backref.label_contents is provided, falls back to `\\hyperref`"""

    description = "\\cref"

    # TODO more cleveref initialization
    capitalize = True
    nameinref = True

    @override
    def request_packages(self, package_resolver: LatexPackageResolver) -> None:
        package_resolver.request_latex_package("hyperref", "fallback for cleveref")
        cleveref_options = []
        if self.capitalize:
            cleveref_options.append("capitalize")
        if self.nameinref:
            cleveref_options.append("nameinref")
        package_resolver.request_latex_package("cleveref", "backrefs", cleveref_options)

    @override
    def emit_config_for_fmt(
        self, spec: LatexCounterSpec, renderer: LatexRenderer
    ) -> None:
        # TODO there may be a discrepancy between cleveref default counters and the latex/package ones!
        known_to_cleveref = spec.provided_by_docclass_or_package
        if spec.override_fmt or (not known_to_cleveref):
            fmt = spec.get_manual_fmt()
            if not known_to_cleveref:
                renderer.emit_comment_line(
                    "This is a new counter, tell cleveref how to format references to it"
                )
            elif spec.override_fmt:
                renderer.emit_comment_line(
                    "The counter has had its formatting overridden, pass that through to cleveref"
                )
            renderer.emit_macro("crefformat")
            renderer.emit_braced(Raw(spec.latex_counter))
            renderer.emit_braced(
                Text(fmt.name),
                Raw("~#2#1"),
                Text(fmt.postfix_for_end),
                Raw("#3"),
            )
            renderer.emit_break_sentence()

    @override
    def emit_anchor(
        self, anchor: Anchor, renderer: "LatexRenderer", fmt: FmtEnv
    ) -> None:
        renderer.emit_raw(f"\\label{{{anchor.canonical()}}}")

    @override
    def emit_backref(
        self,
        backref: Backref,
        anchor: Anchor,
        renderer: "LatexRenderer",
        _fmt: FmtEnv,
    ) -> None:
        raw_anchor = Raw(anchor.canonical())
        if backref.label_contents:
            renderer.emit_macro("hyperref")
            renderer.emit_sqr_bracketed(raw_anchor)
            renderer.emit_braced(backref.label_contents)
        else:
            renderer.emit_macro("cref")
            renderer.emit_braced(raw_anchor)


class LatexPageRef(LatexBackrefMethodImpl):
    """Cross-referencing for backreferences to non-floating non-counted things, i.e. by page number, using `\\phantomsection\\label` and `\\pageref` or `\\hyperref` when a backref caption is provided."""

    description = "pageref"

    @override
    def request_packages(self, package_resolver: LatexPackageResolver) -> None:
        package_resolver.request_latex_package("hyperref", "fallback for pageref")

    @override
    def emit_config_for_fmt(
        self, _spec: LatexCounterSpec, _renderer: LatexRenderer
    ) -> None:
        pass

    @override
    def emit_anchor(
        self, anchor: Anchor, renderer: "LatexRenderer", _fmt: FmtEnv
    ) -> None:
        renderer.emit_macro("phantomsection")
        renderer.emit_newline()  # TODO not sure this is necessary but it's included in a lot of examples
        renderer.emit_raw(f"\\label{{{anchor.canonical()}}}")

    @override
    def emit_backref(
        self,
        backref: Backref,
        anchor: Anchor,
        renderer: "LatexRenderer",
        _fmt: FmtEnv,
    ) -> None:
        raw_anchor = Raw(anchor.canonical())
        if backref.label_contents:
            renderer.emit_macro("hyperref")
            renderer.emit_sqr_bracketed(raw_anchor)
            renderer.emit_braced(backref.label_contents)
        else:
            renderer.emit(
                Text("page ")
            )  ## hooooo boy yeah this isn't great... capitalization is annoying
            renderer.emit_macro("pageref")
            renderer.emit_braced(raw_anchor)
