import html
from contextlib import contextmanager
from enum import Enum
from typing import Dict, Generator, Iterable, Iterator, List, Optional, Tuple, Type

from turnip_text import Block, DocSegmentHeader, Inline, Paragraph, UnescapedText
from turnip_text.doc import DocAnchors, DocSetup, DocState, FormatContext
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.render import (
    EmitterDispatch,
    RefEmitterDispatch,
    Renderer,
    RenderPlugin,
    RenderSetup,
    VisitorFilter,
    VisitorFunc,
    Writable,
)
from turnip_text.render.counters import (
    CounterLink,
    CounterState,
    build_counter_hierarchy,
)
from turnip_text.render.manual_numbering import (
    ARABIC_NUMBERING,
    LOWER_ALPH_NUMBERING,
    LOWER_ROMAN_NUMBERING,
    UPPER_ALPH_NUMBERING,
    UPPER_ROMAN_NUMBERING,
    ManualNumbering,
    SimpleCounterFormat,
)


class MarkdownCounterStyle(Enum):
    """
    Possible numbering styles for a counter. This is only for the counter number itself, not the surrounding content e.g. dots between numbers.
    """

    Arabic = "arabic"
    AlphLower = "alph"
    AlphUpper = "Alph"
    RomanLower = "roman"
    RomanUpper = "Roman"


STYLE_TO_NUMBERING = {
    MarkdownCounterStyle.Arabic: ARABIC_NUMBERING,
    MarkdownCounterStyle.AlphLower: LOWER_ALPH_NUMBERING,
    MarkdownCounterStyle.AlphUpper: UPPER_ALPH_NUMBERING,
    MarkdownCounterStyle.RomanLower: LOWER_ROMAN_NUMBERING,
    MarkdownCounterStyle.RomanUpper: UPPER_ROMAN_NUMBERING,
}


MarkdownCounterFormatting = SimpleCounterFormat[ManualNumbering]


class MarkdownRenderer(Renderer):
    html_mode_stack: List[bool]
    counters: CounterState
    counter_rendering: Dict[str, MarkdownCounterFormatting]

    def __init__(
        self,
        doc_setup: DocSetup,
        handlers: EmitterDispatch["MarkdownRenderer"],
        counters: CounterState,
        counter_rendering: Dict[str, MarkdownCounterFormatting],
        write_to: Writable,
        html_mode: bool = False,
    ) -> None:
        super().__init__(doc_setup, handlers, write_to)
        self.counters = counters
        self.counter_rendering = counter_rendering
        # Once you're in HTML mode, you can't drop down to Markdown mode again.
        # If they asked for HTML mode only, just make that the first entry in the stack.
        # If they didn't, we start in Markdown mode.
        self.html_mode_stack = [html_mode]

    def emit_unescapedtext(self, t: UnescapedText) -> None:
        if self.in_html_mode:
            self.emit_raw(html.escape(t.text))
        else:
            # note - right now this assumes we're using a unicode-compatible setup and thus don't need to escape unicode characters.
            # note - order is important here because the subsitutions may introduce more special characters.
            # e.g. if the backslash replacement applied later, the backslash from escaping "(" would be escaped as well

            # TODO - some of these are overzealous, e.g. () and -, because in most contexts they're interpreted as normal text.
            # context sensitive escaping?
            # https://www.markdownguide.org/basic-syntax/
            special_chars = r"\\`*_{}[]<>()#+-.!|"

            data = t.text
            for char in special_chars:
                data = data.replace(char, "\\" + char)
            self.emit_raw(data)

    def emit_paragraph(self, p: Paragraph) -> None:
        if self.in_html_mode:
            self.emit_raw("<p>")
            self.emit_newline()
            with self.indent(4):
                super().emit_paragraph(p)
                # emit_paragraph *joins* the sentences, but it doesn't end with a newline
                self.emit_break_sentence()
            self.emit_raw("</p>")
        else:
            super().emit_paragraph(p)

    # TODO do this for blocks instead of paragraphs?
    # def emit_block(self, b: Block) -> None:
    #     if self.in_html_mode:
    #         self.emit_raw("<p>")
    #         with self.indent(4):
    #             self.emit_line_break()
    #             super().emit_block(b)
    #         self.emit_line_break()
    #         self.emit_raw("</p>")
    #     else:
    #         super().emit_block(b)

    @property
    def in_html_mode(self) -> bool:
        return self.html_mode_stack[-1]

    @contextmanager
    def html_mode(self) -> Iterator[None]:
        self.html_mode_stack.append(True)

        try:
            yield
        finally:
            self.html_mode_stack.pop()

    @contextmanager
    def emit_tag(
        self, tag: str, props: str | None = None, indent: int = 0
    ) -> Generator[None, None, None]:
        if not self.in_html_mode:
            raise RuntimeError("Can't emit_tag without going into HTML mode!")

        if props:
            self.emit_raw(f"<{tag} {props}>")
        else:
            self.emit_raw(f"<{tag}>")

        try:
            if indent:
                with self.indent(indent):
                    self.emit_newline()
                    yield
                    self.emit_newline()
            else:
                yield
        finally:
            self.emit_raw(f"</{tag}>")

    def emit_empty_tag(self, tag: str, props: str | None = None) -> None:
        # This is allowed outside of HTML mode because it doesn't contain anything.
        if props:
            self.emit_raw(f"<{tag} {props}></{tag}>")
        else:
            self.emit_raw(f"<{tag}></{tag}>")

    def emit_url(self, url: str, label: Optional[Inline]) -> None:
        if "<" in url or ">" in url or ")" in url or '"' in url:
            raise RuntimeError(
                f"Can't handle url {url} with a <, >, \", or ) in it. Please use proper percent-encoding to escape it."
            )

        if self.in_html_mode:
            assert ">" not in url and "<" not in url and '"' not in url
            self.emit_raw(f'<a href="{url}">')
            if label is None:
                # Set the "name" of the URL to the text of the URL - escaped so it can be read as normal markdown
                self.emit_unescapedtext(UnescapedText(url))
            else:
                self.emit(label)
            self.emit_raw("</a>")
        else:
            assert ")" not in url
            self.emit_raw("[")
            if label is None:
                # Set the "name" of the URL to the text of the URL - escaped so it can be read as normal markdown
                self.emit_unescapedtext(UnescapedText(url))
            else:
                self.emit(label)
            self.emit_raw(f"]({url})")

    def emit_anchor(self, anchor: Anchor) -> None:
        self.emit_empty_tag("a", f'id="{anchor.canonical()}"')

    def emit_backref(self, backref: Backref) -> None:
        anchor = self.anchors.lookup_backref(backref)
        url = f"#{anchor.canonical()}"
        if backref.label_contents:
            self.emit_url(url, backref.label_contents)
        else:
            self.emit_url(url, self.anchor_to_ref_text(anchor))

    def anchor_to_ref_text(self, anchor: Anchor) -> UnescapedText:
        counters = self.counters.anchor_counters[anchor]
        return MarkdownCounterFormatting.resolve(
            [(self.counter_rendering[kind], i) for (kind, i) in counters]
        )

    def anchor_to_number_text(self, anchor: Anchor) -> UnescapedText:
        counters = self.counters.anchor_counters[anchor]
        return MarkdownCounterFormatting.resolve(
            [(self.counter_rendering[kind], i) for (kind, i) in counters],
            with_name=False,
        )

    @classmethod
    def default_emitter_dispatch(
        cls: type["MarkdownRenderer"],
    ) -> EmitterDispatch["MarkdownRenderer"]:
        emitter = super().default_emitter_dispatch()
        emitter.register_block_or_inline(
            Anchor, lambda anchor, renderer, fmt: renderer.emit_anchor(anchor)
        )
        emitter.register_block_or_inline(
            Backref, lambda backref, renderer, fmt: renderer.emit_backref(backref)
        )
        return emitter


class MarkdownSetup(RenderSetup[MarkdownRenderer]):
    html_only: bool
    emitter: EmitterDispatch[MarkdownRenderer]
    counter_rendering: Dict[str, MarkdownCounterFormatting]
    requested_counter_links: List[CounterLink]
    counters: CounterState

    def __init__(
        self,
        plugins: Iterable[RenderPlugin[MarkdownRenderer, "MarkdownSetup"]],
        requested_counter_links: Optional[Iterable[CounterLink]] = None,
        html_only: bool = False,
    ) -> None:
        super().__init__(plugins)
        self.html_only = html_only
        self.emitter = MarkdownRenderer.default_emitter_dispatch()
        self.counter_rendering = {}
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
        counter_format: MarkdownCounterFormatting,
        parent_counter: Optional[str] = None,
    ) -> None:
        """
        Given a counter, define:
        - how it's name is formatted in backreferences
        - what macros are used to backreference the counter
        - what the counter's parent should be.
        """
        # TODO check if we've defined this counter before already

        self.counter_rendering[counter] = counter_format

        # Apply the requested counter links
        # TODO in the not-set case we probably shouldn't do this? but then how would the CounterState know about them
        self.requested_counter_links.append((parent_counter, counter))

    def to_renderer(self, doc_setup: DocSetup, write_to: Writable) -> MarkdownRenderer:
        return MarkdownRenderer(
            doc_setup,
            self.emitter,
            self.counters,
            self.counter_rendering,
            write_to,
            html_mode=self.html_only,
        )


class HtmlSetup(MarkdownSetup):
    def __init__(
        self,
        plugins: Iterable[RenderPlugin[MarkdownRenderer, "MarkdownSetup"]],
        requested_counter_links: Optional[Iterable[CounterLink]] = None,
    ) -> None:
        super().__init__(plugins, requested_counter_links, html_only=True)


MarkdownPlugin = RenderPlugin[MarkdownRenderer, MarkdownSetup]
