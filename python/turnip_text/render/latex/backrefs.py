from enum import IntEnum

from turnip_text import UnescapedText
from turnip_text.doc import FormatContext
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.render.latex.renderer import LatexBackrefMethodImpl, LatexRenderer


class LatexBackrefMethod(IntEnum):
    Cleveref = 0
    Hyperlink = 1
    PageRef = 2


# TODO latex \autoref support?


class LatexHyperlink(LatexBackrefMethodImpl):
    """A means of referring back to a position in the document using the \\hypertarget and \\hyperlink macros instead of \\label and \\XXXref"""

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
        fmt: FormatContext,
    ) -> None:
        renderer.emit_macro("hyperlink")
        renderer.emit_braced(anchor.canonical())
        assert (
            backref.label_contents is not None
        ), "Can't emit a backreferences as a \\hyperlink without a label/caption"
        # TODO enable using LatexCounterFormat to compute this
        renderer.emit_braced(backref.label_contents)


class LatexCleveref(LatexBackrefMethodImpl):
    """Cross-referencing using the `cleveref` LaTeX package, which has built-in sane defaults for many counters and languages. If backref.label_contents is provided, falls back to `\\hyperref`"""

    # TODO more cleveref initialization
    capitalize = True
    nameinref = True

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
        fmt: FormatContext,
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

    def emit_anchor(
        self, anchor: Anchor, renderer: "LatexRenderer", fmt: FormatContext
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
        fmt: FormatContext,
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
