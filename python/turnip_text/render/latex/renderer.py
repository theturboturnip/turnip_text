import abc
from contextlib import contextmanager
from dataclasses import dataclass
from enum import Enum, IntEnum
from typing import Any, Dict, Iterable, Iterator, List, Optional, Tuple, Type, Union

from turnip_text import Block, DocSegmentHeader, Inline, UnescapedText
from turnip_text.doc import DocSetup, FormatContext
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.render import (
    EmitterDispatch,
    Renderer,
    RendererSetup,
    RenderPlugin,
    VisitorFilter,
    VisitorFunc,
    Writable,
)
from turnip_text.render.counters import (
    CounterLink,
    CounterState,
    build_counter_hierarchy,
)


class LatexCounterStyle(Enum):
    """
    Possible numbering styles for a counter. This is only for the counter number itself, not the surrounding content e.g. dots between numbers.

    TODO use this for something

    See https://en.wikibooks.org/wiki/LaTeX/Counters#Counter_style
    """

    Arabic = "arabic"
    AlphLower = "alph"
    AlphUpper = "Alph"
    RomanLower = "roman"
    RomanUpper = "Roman"
    Symbol = "fnsymbol"


class LatexBackrefMethodImpl(abc.ABC):
    @abc.abstractmethod
    def emit_anchor(
        self, anchor: Anchor, renderer: "LatexRenderer", fmt: FormatContext
    ) -> None:
        ...

    @abc.abstractmethod
    def emit_backref(
        self,
        backref: Backref,
        anchor: Anchor,
        renderer: "LatexRenderer",
        fmt: FormatContext,
    ) -> None:
        ...


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


class LatexRenderer(Renderer):
    anchor_kind_to_backref_method: Dict[str, LatexBackrefMethodImpl]

    def __init__(
        self,
        doc_setup: DocSetup,
        handlers: EmitterDispatch["LatexRenderer"],
        anchor_kind_to_backref_method: Dict[str, LatexBackrefMethodImpl],
        write_to: Writable,
    ) -> None:
        super().__init__(doc_setup, handlers, write_to)
        self.anchor_kind_to_backref_method = anchor_kind_to_backref_method

    def emit_unescapedtext(self, t: UnescapedText) -> None:
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
        self.anchor_kind_to_backref_method[anchor.kind].emit_anchor(
            anchor, self, self.fmt
        )

    def emit_backref(self, backref: Backref) -> None:
        anchor = self.anchors.lookup_backref(backref)
        self.anchor_kind_to_backref_method[anchor.kind].emit_backref(
            backref, anchor, self, self.fmt
        )

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


# TODO this is a great place to put in stuff for calculating the preamble!
class LatexSetup(RendererSetup[LatexRenderer]):
    emitter: EmitterDispatch[LatexRenderer]
    counter_kind_to_backref_method: Dict[str, Optional[LatexBackrefMethod]]
    backref_impls: Dict[LatexBackrefMethod, LatexBackrefMethodImpl]
    requested_counter_links: List[CounterLink]
    counters: CounterState

    def __init__(
        self,
        plugins: Iterable[RenderPlugin[LatexRenderer, "LatexSetup"]],
        requested_counter_links: Optional[Iterable[CounterLink]] = None,
        # TODO config for the backref methods
    ) -> None:
        super().__init__(plugins)
        self.emitter = LatexRenderer.default_emitter_dispatch()
        self.counter_kind_to_backref_method = {}
        self.backref_impls = {
            # TODO make sure we load hyperref and cleveref!
            # TODO make loading cleveref optional
            LatexBackrefMethod.Cleveref: LatexCleveref(),
            LatexBackrefMethod.Hyperlink: LatexHyperlink(),
            LatexBackrefMethod.PageRef: LatexPageRef(),
        }
        if requested_counter_links:
            self.requested_counter_links = list(requested_counter_links)
        else:
            self.requested_counter_links = []
        # This allows plugins to register with the emitter and request specific counter links
        for p in plugins:
            p._register(self)
        # Now we know the full hierarchy we can build the CounterState
        self.counters = CounterState(
            build_counter_hierarchy(self.requested_counter_links)
        )

    def gen_dfs_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]]:
        vs: List[Tuple[VisitorFilter, VisitorFunc]] = [
            (None, self.counters.count_anchor_if_present)
        ]
        for p in self.plugins:
            v = p._make_visitors()
            if v:
                vs.extend(v)
        return vs

    def known_node_types(
        self,
    ) -> Iterable[type[Block] | type[Inline] | type[DocSegmentHeader]]:
        return self.emitter.renderer_keys()

    def known_countables(self) -> Iterable[str]:
        return self.counters.anchor_kind_to_parent_chain.keys()

    def define_counter_rendering(
        self,
        counter: str,
        # counter_format: Optional[LatexCounterFormat],
        backref_method: Union[
            None, LatexBackrefMethod, Tuple[LatexBackrefMethod, ...]
        ],  # Either one or multiple possible backref methods. If a tuple, the first element that is present in self.backref_impls will be selected
        parent_counter: Optional[str] = None,
    ):
        """
        Given a counter, define:
        - how it's name is formatted in backreferences
        - what macros are used to backreference the counter
        - what the counter's parent should be.
        """
        # TODO check if we've defined this counter before already

        # Figure out which backref_method we can use
        if backref_method is not None:
            if isinstance(backref_method, LatexBackrefMethod):
                backref_methods: Tuple[LatexBackrefMethod, ...] = (backref_method,)
            else:
                backref_methods = backref_method
            found_valid_method = False
            for backref_method in backref_methods:
                if backref_method in self.backref_impls:
                    self.counter_kind_to_backref_method[counter] = backref_method
                    found_valid_method = True
                    break
            if not found_valid_method:
                raise ValueError(
                    f"None of the supplied backref methods {backref_methods} for counter '{counter}' were available in the document. Available methods: {self.backref_impls.keys()}"
                )
        else:
            self.counter_kind_to_backref_method[counter] = None

        # Apply the requested counter links
        # TODO in the not-set case we probably shouldn't do this? but then how would the CounterState know about them
        self.requested_counter_links.append((parent_counter, counter))

    def to_renderer(self, doc_setup: DocSetup, write_to: Writable) -> LatexRenderer:
        return LatexRenderer(
            doc_setup,
            self.emitter,
            {
                counter: self.backref_impls[method]
                for counter, method in self.counter_kind_to_backref_method.items()
                if method is not None
            },
            write_to,
        )


LatexPlugin = RenderPlugin[LatexRenderer, LatexSetup]
