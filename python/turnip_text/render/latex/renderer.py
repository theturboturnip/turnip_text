import abc
from contextlib import contextmanager
from dataclasses import dataclass
from enum import Enum
from typing import Any, Dict, Iterator, List, Optional, Sequence, Set, Tuple

from turnip_text import DocSegment, Text
from turnip_text.doc import DocSetup, FormatContext
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.helpers import MaybeUnset
from turnip_text.render import EmitterDispatch, Renderer, Writable
from turnip_text.render.counters import CounterChainValue, CounterState
from turnip_text.render.manual_numbering import (
    ARABIC_NUMBERING,
    LOWER_ALPH_NUMBERING,
    LOWER_ROMAN_NUMBERING,
    UPPER_ALPH_NUMBERING,
    UPPER_ROMAN_NUMBERING,
    BasicManualNumbering,
    ManualNumbering,
    SimpleCounterFormat,
)


class LatexCounterStyle(Enum):
    """
    Possible numbering styles for a counter. This is only for the counter number itself, not the surrounding content e.g. dots between numbers.

    See https://en.wikibooks.org/wiki/LaTeX/Counters#Counter_style
    """

    Arabic = "arabic"
    AlphLower = "alph"
    AlphUpper = "Alph"
    RomanLower = "roman"
    RomanUpper = "Roman"
    Symbol = "fnsymbol"

    def __getitem__(self, num: int) -> str:
        return COUNTER_STYLE_TO_MANUAL[self][num]


# TODO use latex \symbols, not unicode chars
LATEX_SYMBOL_NUMBERING = BasicManualNumbering(
    [
        "",
        "∗",
        "†",
        "‡",
        "§",
        "¶",
        "∥",
        "∗∗",
        "††",
        "‡‡",
    ]
)

COUNTER_STYLE_TO_MANUAL = {
    LatexCounterStyle.Arabic: ARABIC_NUMBERING,
    LatexCounterStyle.AlphLower: LOWER_ALPH_NUMBERING,
    LatexCounterStyle.AlphUpper: UPPER_ALPH_NUMBERING,
    LatexCounterStyle.RomanLower: LOWER_ROMAN_NUMBERING,
    LatexCounterStyle.RomanUpper: UPPER_ROMAN_NUMBERING,
    LatexCounterStyle.Symbol: LATEX_SYMBOL_NUMBERING,
}


LatexCounterFormat = SimpleCounterFormat[LatexCounterStyle]


# A class that emits anchors and backrefs in a specific way.
# The renderer receives a mapping of (anchor kind) -> (backref method impl) from the LatexSetup.
# Implementations are stored in backrefs.py
class LatexBackrefMethodImpl(abc.ABC):
    description: str

    @abc.abstractmethod
    def emit_config_for_fmt(
        self,
        spec: "LatexCounterSpec",
        renderer: "LatexRenderer",
    ) -> None: ...

    @abc.abstractmethod
    def emit_anchor(
        self,
        anchor: Anchor,
        renderer: "LatexRenderer",
        fmt: FormatContext,
    ) -> None: ...

    @abc.abstractmethod
    def emit_backref(
        self,
        backref: Backref,
        anchor: Anchor,
        renderer: "LatexRenderer",
        fmt: FormatContext,
    ) -> None: ...


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


@dataclass
class LatexCounterSpec:
    """The specification for a Latex counter"""

    latex_counter: str
    tt_counter: Optional[str]

    provided_by_docclass_or_package: bool
    """Does any package or the documentclass define this counter already? If not, we need to declare it with newcounter"""

    default_reset_latex_counter: Optional[str]
    """If this was provided_by_docclass_or_package, what is the standard 'reset counter' for this counter?"""
    reset_latex_counter: Optional[str]

    fallback_fmt: LatexCounterFormat
    override_fmt: Optional[LatexCounterFormat]

    # TODO should this be optional?
    backref_impl: Optional[LatexBackrefMethodImpl]

    def get_manual_fmt(self) -> LatexCounterFormat:
        if self.override_fmt:
            return self.override_fmt
        return self.fallback_fmt


@dataclass
class LatexRequirements:
    document_class: Optional[str]  # May be None if the document is not standalone
    shell_escape: List[str]
    packages: Dict[str, LatexPackageRequirements]
    tt_counter_to_latex: Dict[str, LatexCounterSpec]
    """A mapping of (turnip_text counter) -> LatexCounterSpec for the LaTeX counter mapping to that turnip_text counter. No restrictions on ordering."""
    latex_counter_to_latex: Dict[str, LatexCounterSpec]
    """A mapping of LaTeX counter -> its LatexCounterSpec. Must iterate in a hierarchy compatible order i.e. if x is a reset counter for y then x must appear before y."""
    magic_tt_counters: Dict[str, str]
    """A mapping of (turnip_text counter) -> (magic LaTeX counter). Magic LaTeX counters are incremented in a way turnip_text cannot predict."""

    # TODO fixup package order


class LatexRenderer(Renderer):
    requirements: LatexRequirements
    tt_counters: CounterState

    def __init__(
        self,
        doc_setup: DocSetup,
        requirements: LatexRequirements,
        tt_counters: CounterState,
        handlers: EmitterDispatch["LatexRenderer"],
        # TODO config option to write the full counter setup regardless of what we think the defaults are?
        write_to: Writable,
    ) -> None:
        super().__init__(doc_setup, handlers, write_to)
        self.requirements = requirements
        self.tt_counters = tt_counters

    def emit_document(self, doc: DocSegment) -> None:
        self.emit_comment_headline(
            "Auto-generated by turnip_text: https://github.com/theturboturnip/turnip_text"
        )
        if self.requirements.shell_escape:
            self.emit_comment_headline(
                f"Requires `--shell-escape` command-line option for {', '.join(self.requirements.shell_escape)}"
            )
        if self.requirements.document_class:
            self.emit_macro("documentclass")
            self.emit_braced(self.requirements.document_class)
            self.emit_break_paragraph()
            for package, reason in self.requirements.packages.items():
                self.emit_comment_line(f"{reason.package} required for")
                for r in reason.reasons:
                    self.emit_comment_line(f"- {r}")
                self.emit_macro("usepackage")
                if reason.options:
                    self.emit_sqr_bracketed(",".join(reason.options))
                self.emit_braced(reason.package)
                self.emit_break_sentence()

            self.emit_break_paragraph()
            self.emit_comment_headline("Configuring counters...")
            tt_counter: Optional[str]
            latex_counter: str
            # Note magic counters, which LaTeX steps automatically and turnip_text cannot imitate
            for (
                tt_counter,
                latex_counter,
            ) in self.requirements.magic_tt_counters.items():
                self.emit_comment_line(
                    f"turnip_text counter '{tt_counter}' maps to magic LaTeX '{latex_counter}' which turnip_text cannot predict"
                )
            # Handle not-magic counters
            for (
                latex_counter,
                latex_counter_spec,
            ) in self.requirements.latex_counter_to_latex.items():
                tt_counter = latex_counter_spec.tt_counter
                if latex_counter_spec.provided_by_docclass_or_package:
                    if tt_counter:
                        self.emit_comment_line(
                            f"turnip_text counter '{tt_counter}' maps to LaTeX '{latex_counter_spec.latex_counter}', uses reset counter '{latex_counter_spec.reset_latex_counter}'"
                        )
                    else:
                        self.emit_comment_line(
                            f"LaTeX counter '{latex_counter}' is known but doesn't map to a turnip_text counter, uses reset counter '{latex_counter_spec.reset_latex_counter}'"
                        )
                    if (
                        latex_counter_spec.reset_latex_counter
                        != latex_counter_spec.default_reset_latex_counter
                    ):
                        if latex_counter_spec.reset_latex_counter is None:
                            # counterwithout = pass in (slave counter) (old master counter) and it will undo the connection to (old master counter)
                            self.emit_macro("counterwithout")
                            self.emit_braced(latex_counter_spec.latex_counter)
                            self.emit_braced(
                                latex_counter_spec.default_reset_latex_counter
                            )
                        else:
                            # counterwithin = pass in (slave counter) (new master counter) and it will set the connection to (new master counter)
                            self.emit_macro("counterwithin")
                            self.emit_braced(latex_counter_spec.latex_counter)
                            self.emit_braced(latex_counter_spec.reset_latex_counter)
                else:
                    if tt_counter:
                        self.emit_comment_line(
                            f"turnip_text counter '{tt_counter}' maps to a new LaTeX counter '{latex_counter_spec.latex_counter}'"
                        )
                    else:
                        self.emit_comment_line(
                            f"LaTeX counter '{latex_counter}' is created but doesn't map to a turnip_text counter"
                        )
                    self.emit_macro("newcounter")
                    self.emit_braced(latex_counter_spec.latex_counter)
                    if latex_counter_spec.reset_latex_counter:
                        self.emit_sqr_bracketed(latex_counter_spec.reset_latex_counter)

                # Setup counter numbering
                # A counter's formatting in LaTeX is ({parent counter}{parent counter.postfix_for_child}{numbering(this counter)})
                # By default, LaTeX uses {parent counter}.{arabic(this counter)}
                # => the counter's formatting will get updated if
                # - it uses non-arabic numbering
                # - it's parent doesn't use a period as a postfix-for-child
                reasons_to_reset_counter_fmt = []
                latex_counter_fmt = latex_counter_spec.get_manual_fmt()
                if latex_counter_fmt.style != LatexCounterStyle.Arabic:
                    reasons_to_reset_counter_fmt.append(
                        f"it uses non-default numbering {latex_counter_fmt.style.value}"
                    )

                reset_counter_fmt = (
                    self.requirements.latex_counter_to_latex[
                        latex_counter_spec.reset_latex_counter
                    ].get_manual_fmt()
                    if latex_counter_spec.reset_latex_counter
                    else None
                )
                if reset_counter_fmt and reset_counter_fmt.postfix_for_child != ".":
                    reasons_to_reset_counter_fmt.append(
                        f"parent counter '{latex_counter_spec.reset_latex_counter}' uses a non-default parent-child separator '{reset_counter_fmt.postfix_for_child}'"
                    )

                if reasons_to_reset_counter_fmt:
                    self.emit_comment_line(
                        f"Redefining format for '{latex_counter}' because {','.join(reasons_to_reset_counter_fmt)}"
                    )
                    fmt = latex_counter_spec.get_manual_fmt()
                    self.emit_macro("renewcommand")
                    self.emit(f"{{\\the{latex_counter}}}{{")
                    if reset_counter_fmt:
                        self.emit(
                            f"\\the{latex_counter_spec.reset_latex_counter}{{}}",
                            Text(reset_counter_fmt.postfix_for_child),
                        )
                    self.emit_macro(fmt.style.value)
                    self.emit_braced(latex_counter)
                    # Do not apply fmt.postfix_for_end here - if you do, it'll get lumped in with children
                    self.emit("}}\n")

                backref_impl = latex_counter_spec.backref_impl
                if backref_impl:
                    self.emit_comment_line(
                        f"LaTeX counter '{latex_counter_spec.latex_counter}' is backreferenced with {backref_impl.description}"
                    )
                    backref_impl.emit_config_for_fmt(
                        latex_counter_spec,
                        self,
                    )
                else:
                    self.emit_comment_line(
                        f"LaTeX counter '{latex_counter_spec.latex_counter}' is not backreferenced."
                    )

            self.emit_comment_headline("...done configuring counters")

            self.emit_break_paragraph()
            with self.emit_env("document", indent=0):
                self.emit_segment(doc)
        else:
            self.emit_comment_headline("Required packages:")
            for package, reason in self.requirements.packages.items():
                self.emit_comment_line(reason.as_latex_preamble_comment())

            self.emit_segment(doc)

    # TODO override emit_sentence to get sentence-break-whitespace at the end of each sentence?

    def emit_unescapedtext(self, t: Text) -> None:
        # TODO make sure whitespace we emit here *isn't* sentence break whitespace?

        # note - right now this assumes we're using a unicode-compatible setup and thus don't need to escape unicode characters.
        # note - order is important here because the subsitutions may introduce more special characters.
        # e.g. if the backslash replacement applied later, the backslash from escaping "%" would be replaced with \textbackslash
        ascii_map = {
            "\\": "\\textbackslash{}",
            "%": "\%",
            "$": "\$",
            "{": "\{",
            "}": "\}",
            "_": "\_",
            "#": "\#",
            "&": "\&",
            "~": "\~{}",
        }
        data = t.text
        for c, replace_with in ascii_map.items():
            data = data.replace(c, replace_with)
        self.emit_raw(data)

    def emit_macro(self, name: str) -> None:
        self.emit_raw(f"\\{name}")

    def emit_sqr_bracketed(self, *args: Any) -> None:
        self.emit_raw("[")
        self.emit(*args)
        self.emit_raw("]")

    def emit_braced(self, *args: Any) -> None:
        self.emit_raw("{")
        self.emit(*args)
        self.emit_raw("}")

    def emit_comment_line(self, content: str) -> None:
        content = content.strip()
        if "\n" in content:
            raise ValueError("Can't put a newline inside a comment")
        self.emit_raw(f"% {content}\n")

    def emit_comment_headline(self, content: str) -> None:
        content = content.strip()
        if "\n" in content:
            raise ValueError("Can't put a newline inside a comment")
        self.emit_raw(f"%%% {content}\n")

    @contextmanager
    def emit_env(self, name: str, indent: int = 4) -> Iterator[None]:
        self.emit_macro("begin")
        self.emit_braced(name)
        self.push_indent(indent)
        self.emit_break_sentence()

        try:
            yield
        finally:
            self.pop_indent(indent)
            self.emit_break_sentence()
            self.emit_macro("end")
            self.emit_braced(name)

    def emit_anchor(self, anchor: Anchor) -> None:
        backref_method = self.requirements.tt_counter_to_latex[anchor.kind].backref_impl
        if backref_method is None:
            raise ValueError(
                f"Cannot backreference anchor {anchor.kind} - no method specified to display it"
            )
        backref_method.emit_anchor(anchor, self, self.fmt)

    def emit_backref(self, backref: Backref) -> None:
        anchor = self.anchors.lookup_backref(backref)
        backref_method = self.requirements.tt_counter_to_latex[anchor.kind].backref_impl
        if backref_method is None:
            raise ValueError(
                f"Cannot backreference anchor {anchor.kind} - no method specified to display it"
            )
        backref_method.emit_backref(backref, anchor, self, self.fmt)

    def get_resolved_anchor_text(self, anchor: Anchor) -> Text:
        counter_to_resolve = [
            (
                self.requirements.tt_counter_to_latex[kind].get_manual_fmt(),
                i,
            )
            for (kind, i) in self.tt_counters.anchor_counters[anchor]
        ]
        return SimpleCounterFormat.resolve(counter_to_resolve)

    @classmethod
    def default_emitter_dispatch(
        cls: type["LatexRenderer"],
    ) -> EmitterDispatch["LatexRenderer"]:
        emitter = super().default_emitter_dispatch()
        emitter.register_block_or_inline(
            Anchor, lambda anchor, renderer, _: renderer.emit_anchor(anchor)
        )
        emitter.register_block_or_inline(
            Backref, lambda backref, renderer, _: renderer.emit_backref(backref)
        )
        return emitter
