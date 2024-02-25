from contextlib import contextmanager
from typing import Any, Dict, Iterable, Iterator, List, Optional, Tuple, Type

from turnip_text import Block, DocSegmentHeader, Inline, UnescapedText
from turnip_text.doc import DocSetup
from turnip_text.render import (
    EmitterDispatch,
    Renderer,
    RendererSetup,
    RenderPlugin,
    Writable,
)
from turnip_text.render.counters import (
    CounterLink,
    CounterState,
    build_counter_hierarchy,
)


class LatexRenderer(Renderer):
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


# TODO this is a great place to put in stuff for calculating the preamble!
class LatexSetup(RendererSetup[LatexRenderer]):
    emitter: EmitterDispatch[LatexRenderer]
    requested_counter_links: List[CounterLink]
    counters: CounterState

    def __init__(
        self,
        plugins: Iterable[RenderPlugin[LatexRenderer, "LatexSetup"]],
        requested_counter_links: Optional[Iterable[CounterLink]] = None,
    ) -> None:
        super().__init__(plugins)
        self.emitter = LatexRenderer.default_emitter_dispatch()
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

    def known_node_types(
        self,
    ) -> Iterable[type[Block] | type[Inline] | type[DocSegmentHeader]]:
        return self.emitter.renderer_keys()

    def request_counter_links(self, *new_links: CounterLink):
        self.requested_counter_links.extend(new_links)

    def to_renderer(self, doc_setup: DocSetup, write_to: Writable) -> LatexRenderer:
        return LatexRenderer(doc_setup, self.emitter, write_to)


LatexPlugin = RenderPlugin[LatexRenderer, LatexSetup]
