import abc
from contextlib import contextmanager
from enum import Enum
from typing import Any, Dict, Iterator

from turnip_text import UnescapedText
from turnip_text.doc import DocSetup, FormatContext
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.render import EmitterDispatch, Renderer, Writable


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


# A class that emits anchors and backrefs in a specific way.
# The renderer receives a mapping of (anchor kind) -> (backref method impl) from the LatexSetup.
# Implementations are stored in backrefs.py
class LatexBackrefMethodImpl(abc.ABC):
    @abc.abstractmethod
    def emit_anchor(
        self, anchor: Anchor, renderer: "LatexRenderer", fmt: FormatContext
    ) -> None: ...

    @abc.abstractmethod
    def emit_backref(
        self,
        backref: Backref,
        anchor: Anchor,
        renderer: "LatexRenderer",
        fmt: FormatContext,
    ) -> None: ...


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

    # TODO override emit_sentence to get sentence-break-whitespace at the end of each sentence?

    def emit_unescapedtext(self, t: UnescapedText) -> None:
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
