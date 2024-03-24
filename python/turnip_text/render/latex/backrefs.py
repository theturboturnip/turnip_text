from enum import IntEnum
from typing import Dict

from turnip_text import UnescapedText
from turnip_text.doc import FormatContext
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.render.latex.renderer import (
    LatexBackrefMethodImpl,
    LatexCounterSpec,
    LatexRenderer,
)
from turnip_text.render.manual_numbering import SimpleCounterFormat


class LatexBackrefMethod(IntEnum):
    Cleveref = 0
    Hyperlink = 1
    PageRef = 2


# TODO latex \autoref support?
# TODO proper capitalization at the start of sentences


class LatexHyperlink(LatexBackrefMethodImpl):
    """A means of referring back to a position in the document using the \\hypertarget and \\hyperlink macros instead of \\label and \\XXXref"""

    manual_counter_method: Dict[str, SimpleCounterFormat]

    description: str = "\\hypertarget and \\hyperlink"

    def __init__(self) -> None:
        super().__init__()
        self.manual_counter_method = {}

    def emit_config_for_fmt(
        self, spec: LatexCounterSpec, _renderer: LatexRenderer
    ) -> None:
        self.manual_counter_method[spec.latex_counter] = spec.get_manual_fmt()

    def emit_anchor(
        self, anchor: Anchor, renderer: "LatexRenderer", fmt: FormatContext
    ) -> None:
        renderer.emit_macro("hypertarget")
        renderer.emit_braced(anchor.canonical())
        renderer.emit_braced("")  # TODO include caption for anchor?

    def emit_backref(
        self,
        backref: Backref,
        anchor: Anchor,
        renderer: "LatexRenderer",
        _fmt: FormatContext,
    ) -> None:
        renderer.emit_macro("hyperlink")
        renderer.emit_braced(anchor.canonical())
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

    def emit_config_for_fmt(
        self, spec: LatexCounterSpec, renderer: LatexRenderer
    ) -> None:
        # TODO there may be a discrepancy between cleveref default counters and the latex/package ones!
        known_to_cleveref = spec.provided_by_docclass_or_package
        if spec.override_fmt or (not known_to_cleveref):
            fmt = spec.get_manual_fmt()
            renderer.emit_macro("crefformat")
            renderer.emit_braced(spec.latex_counter)
            renderer.emit_braced(
                UnescapedText(fmt.name),
                "~#2#1",
                UnescapedText(fmt.postfix_for_end),
                "#3",
            )

    def emit_anchor(
        self, anchor: Anchor, renderer: "LatexRenderer", fmt: FormatContext
    ) -> None:
        renderer.emit_macro("label")
        renderer.emit_braced(anchor.canonical())

    def emit_backref(
        self,
        backref: Backref,
        anchor: Anchor,
        renderer: "LatexRenderer",
        _fmt: FormatContext,
    ) -> None:
        if backref.label_contents:
            renderer.emit_macro("hyperref")
            renderer.emit_sqr_bracketed(anchor.canonical())
            renderer.emit_braced(backref.label_contents)
        else:
            renderer.emit_macro("cref")
            renderer.emit_braced(anchor.canonical())


class LatexPageRef(LatexBackrefMethodImpl):
    """Cross-referencing for backreferences to non-floating non-counted things, i.e. by page number, using `\\phantomsection\\label` and `\\pageref` or `\\hyperref` when a backref caption is provided."""

    description = "pageref"

    def emit_config_for_fmt(
        self, _spec: LatexCounterSpec, _renderer: LatexRenderer
    ) -> None:
        pass

    def emit_anchor(
        self, anchor: Anchor, renderer: "LatexRenderer", _fmt: FormatContext
    ) -> None:
        renderer.emit_macro("phantomsection")
        renderer.emit_newline()  # TODO not sure this is necessary but it's included in a lot of examples
        renderer.emit_macro("label")
        renderer.emit_braced(anchor.canonical())

    def emit_backref(
        self,
        backref: Backref,
        anchor: Anchor,
        renderer: "LatexRenderer",
        _fmt: FormatContext,
    ) -> None:
        if backref.label_contents:
            renderer.emit_macro("hyperref")
            renderer.emit_sqr_bracketed(anchor.canonical())
            renderer.emit_braced(backref.label_contents)
        else:
            renderer.emit(
                UnescapedText("page ")
            )  ## hooooo boy yeah this isn't great... capitalization is annoying
            renderer.emit_macro("pageref")
            renderer.emit_braced(backref.label_contents)
