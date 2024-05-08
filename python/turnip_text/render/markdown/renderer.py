import html
from contextlib import contextmanager
from enum import Enum
from typing import Dict, Generator, Iterable, Iterator, List, Optional, Tuple

from turnip_text import Block, Document, Header, Inline, Paragraph, Text
from turnip_text.build_system import BuildSystem, JobInputFile, JobOutputFile
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.doc.dfs import VisitorFilter, VisitorFunc
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.anchors import StdAnchorPlugin
from turnip_text.render import (
    EmitterDispatch,
    Renderer,
    RenderPlugin,
    RenderSetup,
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

    def __getitem__(self, num: int) -> str:
        return COUNTER_STYLE_TO_MANUAL[self][num]


COUNTER_STYLE_TO_MANUAL = {
    MarkdownCounterStyle.Arabic: ARABIC_NUMBERING,
    MarkdownCounterStyle.AlphLower: LOWER_ALPH_NUMBERING,
    MarkdownCounterStyle.AlphUpper: UPPER_ALPH_NUMBERING,
    MarkdownCounterStyle.RomanLower: LOWER_ROMAN_NUMBERING,
    MarkdownCounterStyle.RomanUpper: UPPER_ROMAN_NUMBERING,
}


MarkdownCounterFormat = SimpleCounterFormat[MarkdownCounterStyle]


class MarkdownRenderer(Renderer):
    html_mode_stack: List[bool]
    counters: CounterState
    counter_rendering: Dict[str, MarkdownCounterFormat]

    def __init__(
        self,
        fmt: FmtEnv,
        anchors: StdAnchorPlugin,
        handlers: EmitterDispatch["MarkdownRenderer"],
        counters: CounterState,
        counter_rendering: Dict[str, MarkdownCounterFormat],
        write_to: Writable,
        html_mode: bool = False,
    ) -> None:
        super().__init__(fmt, anchors, handlers, write_to)
        self.counters = counters
        self.counter_rendering = counter_rendering
        # Once you're in HTML mode, you can't drop down to Markdown mode again.
        # If they asked for HTML mode only, just make that the first entry in the stack.
        # If they didn't, we start in Markdown mode.
        self.html_mode_stack = [html_mode]

    def emit_unescapedtext(self, t: Text) -> None:
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

    # Surround paragraphs with <p> where applicable in HTML mode.
    # Blocks in general do not need to be surrounded with <p>, but can choose to if they want.
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

        if not self.in_html_mode:
            # We're in markdown mode, see if we can do <url> syntax
            # That depends on not having a label, and the url not having a > which would close it early
            # (which would be fixed by URL ampersend-quoting or percent-encoding,
            # but I doubt I can rely on markdown parsers handling that properly)
            if label is None and ">" not in url:
                self.emit_raw(f"<{url}>")
                return
            # Ok, we can't do that - see if we can do [name](url) syntax
            # That depends on the url not having a ) which would close it early
            elif ")" not in url:
                self.emit_raw("[")
                # The label could still be None here if ">" is in a url
                if label is None:
                    self.emit_unescapedtext(Text(url))
                else:
                    self.emit(label)
                self.emit_raw(f"]({url})")
                return

        # In all other cases, do HTML <a> urls, which have a reliable escape method but aren't pretty
        with self.html_mode():
            self.emit_raw(f'<a href="{html.escape(url)}">')
            if label is None:
                # Set the "name" of the URL to the text of the URL
                self.emit_unescapedtext(Text(url))
            else:
                self.emit(label)
            self.emit_raw("</a>")

    def emit_anchor(self, anchor: Anchor) -> None:
        self.emit_empty_tag("a", f'id="{html.escape(anchor.canonical())}"')

    def emit_backref(self, backref: Backref) -> None:
        anchor = self.anchors.lookup_backref(backref)
        url = f"#{anchor.canonical()}"
        if backref.label_contents:
            self.emit_url(url, backref.label_contents)
        else:
            self.emit_url(url, self.anchor_to_ref_text(anchor))

    def anchor_to_ref_text(self, anchor: Anchor) -> Text:
        counters = self.counters.anchor_counters[anchor]
        return SimpleCounterFormat.resolve(
            [(self.counter_rendering[kind], i) for (kind, i) in counters]
        )

    def anchor_to_number_text(self, anchor: Anchor) -> Text:
        counters = self.counters.anchor_counters[anchor]
        return SimpleCounterFormat.resolve(
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
    counter_rendering: Dict[str, MarkdownCounterFormat]
    requested_counter_links: List[CounterLink]
    counters: CounterState

    def __init__(
        self,
        plugins: Iterable[RenderPlugin["MarkdownSetup"]],
        requested_counter_formatting: Dict[str, MarkdownCounterFormat] = {},
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
        for counter, counter_format in requested_counter_formatting.items():
            self.define_counter_rendering(counter, counter_format)
        # This allows plugins to register with the emitter and request specific counter links
        for p in plugins:
            p._register(self)
        # Now we know the full hierarchy we can build the CounterState
        self.counters = CounterState(
            build_counter_hierarchy(
                self.requested_counter_links, set(self.counter_rendering.keys())
            ),
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
    ) -> Iterable[type[Block] | type[Inline] | type[Header]]:
        return self.emitter.renderer_keys()

    def known_countables(self) -> Iterable[str]:
        return self.counters.anchor_kind_to_parent_chain.keys()

    def define_counter_rendering(
        self,
        counter: str,
        counter_format: MarkdownCounterFormat,
    ) -> None:
        """
        Given a counter, define:
        - how it's name is formatted in backreferences
        - what macros are used to backreference the counter
        """
        if counter not in self.counter_rendering:
            self.counter_rendering[counter] = counter_format

    def request_counter_parent(
        self, counter: str, parent_counter: Optional[str]
    ) -> None:
        # Apply the requested counter links
        self.requested_counter_links.append((parent_counter, counter))

    def register_file_generator_jobs(
        self,
        fmt: FmtEnv,
        anchors: StdAnchorPlugin,
        document: Document,
        build_sys: BuildSystem,
        output_file_name: Optional[str],
    ) -> None:
        # Make a render job and register it in the build system.
        def render_job(_ins: Dict[str, JobInputFile], out: JobOutputFile) -> None:
            with out.open_write_text() as write_to:
                renderer = MarkdownRenderer(
                    fmt,
                    anchors,
                    self.emitter,
                    self.counters,
                    self.counter_rendering,
                    write_to,
                    html_mode=self.html_only,
                )
                renderer.emit_document(document)

        default_output_file_name = "document.html" if self.html_only else "document.md"

        build_sys.register_file_generator(
            render_job,
            inputs={},
            output_relative_path=output_file_name or default_output_file_name,
        )


class HtmlSetup(MarkdownSetup):
    def __init__(
        self,
        plugins: Iterable[RenderPlugin["MarkdownSetup"]],
        requested_counter_formatting: Dict[str, MarkdownCounterFormat] = {},
        requested_counter_links: Optional[Iterable[CounterLink]] = None,
    ) -> None:
        super().__init__(
            plugins,
            requested_counter_formatting,
            requested_counter_links,
            html_only=True,
        )


MarkdownPlugin = RenderPlugin[MarkdownSetup]
