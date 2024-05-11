import abc
from contextlib import contextmanager
from dataclasses import dataclass
from enum import Enum
from typing import Callable, Dict, Iterator, List, Optional, Union

from turnip_text import Block, DocSegment, Document, Inline, Raw, Text
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.anchors import StdAnchorPlugin
from turnip_text.render import EmitterDispatch, Renderer, Writable
from turnip_text.render.counters import CounterState
from turnip_text.render.latex.package_resolver import (
    LatexPackageRequirements,
    LatexPackageResolver,
    ResolvedLatexPackages,
)
from turnip_text.render.manual_numbering import (
    ARABIC_NUMBERING,
    LOWER_ALPH_NUMBERING,
    LOWER_ROMAN_NUMBERING,
    UPPER_ALPH_NUMBERING,
    UPPER_ROMAN_NUMBERING,
    BasicManualNumbering,
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
"""A descripton of how LaTeX renders a counter,
which can be rendered manually by turnip_text or be applied to LaTeX in the preamble."""


# A class that emits anchors and backrefs in a specific way.
# The renderer receives a mapping of (anchor kind) -> (backref method impl) from the LatexSetup.
# Implementations are stored in backrefs.py
class LatexBackrefMethodImpl(abc.ABC):
    description: str

    @abc.abstractmethod
    def request_packages(self, package_resolver: LatexPackageResolver) -> None: ...

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
        fmt: FmtEnv,
    ) -> None: ...

    @abc.abstractmethod
    def emit_backref(
        self,
        backref: Backref,
        anchor: Anchor,
        renderer: "LatexRenderer",
        fmt: FmtEnv,
    ) -> None: ...


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

    default_fmt: LatexCounterFormat
    """If this was provided_by_docclass_or_package, how does LaTeX render it normally?
    Otherwise, how would you like it to be rendered normally"""
    override_fmt: Optional[LatexCounterFormat]

    # TODO should this be optional?
    backref_impl: Optional[LatexBackrefMethodImpl]

    def get_manual_fmt(self) -> LatexCounterFormat:
        if self.override_fmt:
            return self.override_fmt
        return self.default_fmt


@dataclass
class LatexRequirements:
    document_class: Optional[
        str
    ]  # If None, the document is not standalone and shouldn't have a preamble

    shell_escape: List[str]
    packages: List[LatexPackageRequirements]

    preamble_callbacks: List[Callable[["LatexRenderer"], None]]
    """A set of unordered callbacks to emit various components of preamble.
    
    If documentclass is None, these are called at the start of the document (technically not in the preamble.)
    If documentclass is non-None, these are called before emitting \\begin{document}."""

    tt_counter_to_latex: Dict[str, LatexCounterSpec]
    """A mapping of (turnip_text counter) -> LatexCounterSpec for the LaTeX counter mapping to that turnip_text counter. No restrictions on ordering."""
    latex_counter_to_latex: Dict[str, LatexCounterSpec]
    """A mapping of LaTeX counter -> its LatexCounterSpec. Must iterate in a hierarchy compatible order i.e. if x is a reset counter for y then x must appear before y."""
    magic_tt_counter_to_latex_counter: Dict[str, str]
    """A mapping of (turnip_text counter) -> (magic LaTeX counter). Magic LaTeX counters are incremented in a way turnip_text cannot predict."""


class LatexRenderer(Renderer):
    requirements: LatexRequirements
    tt_counters: CounterState

    def __init__(
        self,
        fmt: FmtEnv,
        anchors: StdAnchorPlugin,
        requirements: LatexRequirements,
        tt_counters: CounterState,
        handlers: EmitterDispatch["LatexRenderer"],
        # TODO config option to write the full counter setup regardless of what we think the defaults are?
        write_to: Writable,
    ) -> None:
        super().__init__(fmt, anchors, handlers, write_to)
        self.requirements = requirements
        self.tt_counters = tt_counters

    def emit_document(self, doc: Document) -> None:
        self.emit_comment_headline(
            "Auto-generated by turnip_text: https://github.com/theturboturnip/turnip_text"
        )
        if self.requirements.shell_escape:
            self.emit_comment_headline(
                f"Requires `--shell-escape` command-line option for {', '.join(self.requirements.shell_escape)}"
            )
        if self.requirements.document_class:
            self.emit_raw(f"\\documentclass{{{self.requirements.document_class}}}")
            self.emit_break_paragraph()
            for package in self.requirements.packages:
                self.emit_comment_line(f"{package.package} required for")
                for r in package.reasons:
                    self.emit_comment_line(f"- {r}")
                self.emit_raw(package.as_latex_preamble_line(with_reason=False))
                self.emit_break_sentence()

            self.emit_break_paragraph()
            self.emit_comment_headline("Configuring counters...")
            tt_counter: Optional[str]
            latex_counter: str
            # Note magic counters, which LaTeX steps automatically and turnip_text cannot imitate
            for (
                tt_counter,
                latex_counter,
            ) in self.requirements.magic_tt_counter_to_latex_counter.items():
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
                            self.emit_braced(Raw(latex_counter_spec.latex_counter))
                            # reset_latex_counter is None and default_reset_latex_counter != reset_latex_counter => default_reset_latex_counter is not None
                            self.emit_braced(
                                Raw(
                                    latex_counter_spec.default_reset_latex_counter  # type: ignore[arg-type]
                                )
                            )
                            self.emit_break_sentence()
                        else:
                            # counterwithin = pass in (slave counter) (new master counter) and it will set the connection to (new master counter)
                            self.emit_macro("counterwithin")
                            self.emit_braced(Raw(latex_counter_spec.latex_counter))
                            self.emit_braced(
                                Raw(latex_counter_spec.reset_latex_counter)
                            )
                            self.emit_break_sentence()
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
                    self.emit_braced(Raw(latex_counter_spec.latex_counter))
                    if latex_counter_spec.reset_latex_counter:
                        self.emit_sqr_bracketed(
                            Raw(latex_counter_spec.reset_latex_counter)
                        )
                    self.emit_break_sentence()

                # Setup counter numbering
                # A counter's formatting in LaTeX is ({parent counter}{parent counter.postfix_for_child}{numbering(this counter)})
                # By default, LaTeX uses {parent counter}.{arabic(this counter)}
                # => the counter's formatting will get updated if
                # - it uses numbering that doesn't match the default
                #   - the default numbering for new counters is arabic
                #   - otherwise the default numbering is defined in the counter spec
                # - it's parent doesn't use the default postfix-for-child
                #   - the default for new counters is a period '.'
                #   - otherwise the default is defined in the parent's counter spec
                reasons_to_reset_counter_fmt = []
                latex_counter_fmt = latex_counter_spec.get_manual_fmt()
                default_numbering_style = (
                    latex_counter_spec.default_fmt.style
                    if latex_counter_spec.provided_by_docclass_or_package
                    else LatexCounterStyle.Arabic
                )
                if latex_counter_fmt.style != default_numbering_style:
                    reasons_to_reset_counter_fmt.append(
                        f"it uses non-default numbering {latex_counter_fmt.style.value}"
                    )

                if latex_counter_spec.reset_latex_counter:
                    reset_counter_spec = self.requirements.latex_counter_to_latex[
                        latex_counter_spec.reset_latex_counter
                    ]
                    reset_counter_fmt = reset_counter_spec.get_manual_fmt()
                    # NOTE it's difficult to define the "default" for non-custom counter specs.
                    # Effectively we want "does out reset_counter_spec formatting differ to what LaTeX has"
                    # and if the counter spec is predefined then it could use different separators for different
                    # subcounters
                    # e.g. if B and C both reset on A, B could be defined in LaTeX as A.B and C could be A-C.
                    default_postfix = (
                        reset_counter_spec.default_fmt.postfix_for_child
                        if latex_counter_spec.provided_by_docclass_or_package
                        else "."
                    )
                    if reset_counter_fmt.postfix_for_child != default_postfix:
                        reasons_to_reset_counter_fmt.append(
                            f"parent counter '{latex_counter_spec.reset_latex_counter}' uses a non-default parent-child separator '{reset_counter_fmt.postfix_for_child}'"
                        )
                else:
                    reset_counter_fmt = None

                if reasons_to_reset_counter_fmt:
                    self.emit_comment_line(
                        f"Redefining format for '{latex_counter}' because {','.join(reasons_to_reset_counter_fmt)}"
                    )
                    fmt = latex_counter_spec.get_manual_fmt()
                    # \renewcommamd{\thecounter}{\theresetcounter{}postfix\style{latex_counter}}\n

                    # \renewcommand
                    self.emit_macro("renewcommand")
                    # \renewcommamd{\thecounter}{
                    self.emit_raw(f"{{\\the{latex_counter}}}{{")
                    # \renewcommamd{\thecounter}{\theresetcounter{}postfix
                    if reset_counter_fmt:
                        self.emit(
                            Raw(f"\\the{latex_counter_spec.reset_latex_counter}{{}}"),
                            Text(reset_counter_fmt.postfix_for_child),
                        )
                    # \renewcommamd{\thecounter}{\theresetcounter{}postfix\style
                    self.emit_macro(fmt.style.value)
                    # \renewcommamd{\thecounter}{\theresetcounter{}postfix\style{latex_counter}
                    self.emit_braced(Raw(latex_counter))
                    # Do not apply fmt.postfix_for_end here - if you do, it'll get lumped in with children
                    # \renewcommamd{\thecounter}{\theresetcounter{}postfix\style{latex_counter}}\n
                    self.emit_raw("}\n")

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

            # Emit custom preamble contents in the preamble
            for callback in self.requirements.preamble_callbacks:
                callback(self)
                self.emit_break_paragraph()

            with self.emit_env("document", indent=0):
                super().emit_document(doc)
        else:
            self.emit_comment_headline("Required packages:")
            for package in self.requirements.packages:
                self.emit_comment_line(package.as_latex_preamble_line(with_reason=True))

            # Emit custom preamble contents in the preamble
            for callback in self.requirements.preamble_callbacks:
                callback(self)
                self.emit_break_paragraph()

            super().emit_document(doc)

    # TODO override emit_sentence to get sentence-break-whitespace at the end of each sentence?

    def emit_text(self, t: Text) -> None:
        # TODO make sure whitespace we emit here *isn't* sentence break whitespace?

        # TODO consider using \detokenize?
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
            # Unicode NBSP -> LaTeX ~ NBSP
            "\u00A0": "~",
            # Unicode en, emdashes -> LaTeX dash shortcuts
            "\u2013": "--",
            "\u2014": "---",
        }
        data = t.text
        for c, replace_with in ascii_map.items():
            data = data.replace(c, replace_with)
        self.emit_raw(data)

    def emit_macro(self, name: str) -> None:
        self.emit_raw(f"\\{name}")

    def emit_sqr_bracketed(self, *args: Union[Inline, Block, DocSegment]) -> None:
        self.emit_raw("[")
        self.emit(*args)
        self.emit_raw("]")

    def emit_braced(self, *args: Union[Inline, Block, DocSegment]) -> None:
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
        self.emit_raw(f"\\begin{{{name}}}")
        self.push_indent(indent)
        self.emit_break_sentence()

        try:
            yield
        finally:
            self.pop_indent(indent)
            self.emit_break_sentence()
            self.emit_raw(f"\end{{{name}}}")

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

    def get_anchor_name(self, anchor: Anchor) -> Text:
        return Text(
            self.requirements.tt_counter_to_latex[anchor.kind].get_manual_fmt().name
        )

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
