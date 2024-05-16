from typing import Iterator, List, Tuple, Union

from turnip_text import Blocks, DocSegment, Inlines, Raw, Text, join_inlines
from turnip_text.build_system import BuildSystem
from turnip_text.doc.anchors import Backref
from turnip_text.doc.dfs import VisitorFilter, VisitorFunc
from turnip_text.env_plugins import FmtEnv
from turnip_text.helpers import paragraph_of
from turnip_text.plugins.doc_structure import (
    AppendixHeader,
    BasicHeader,
    StructureEnvPlugin,
    TableOfContents,
    TitleBlock,
)
from turnip_text.render.manual_numbering import SimpleCounterFormat, SimpleCounterStyle
from turnip_text.render.markdown.renderer import (
    MarkdownPlugin,
    MarkdownRenderer,
    MarkdownSetup,
)


class MarkdownStructurePlugin(MarkdownPlugin, StructureEnvPlugin):
    _has_chapter: bool

    def __init__(self, use_chapters: bool) -> None:
        super().__init__()
        self._has_chapter = use_chapters

    def _register(self, build_sys: BuildSystem, setup: MarkdownSetup) -> None:
        setup.emitter.register_block_or_inline(TitleBlock, self._emit_title)
        setup.emitter.register_block_or_inline(TableOfContents, self._emit_toc)
        setup.emitter.register_header(BasicHeader, self._emit_structure)
        setup.emitter.register_header(AppendixHeader, self._emit_appendix)
        setup.define_counter_rendering(
            "h1",
            SimpleCounterFormat(
                name=("chapter" if self._has_chapter else "section"),
                style=SimpleCounterStyle.Arabic,
            ),
        )
        setup.define_counter_rendering(
            "h2",
            SimpleCounterFormat(
                name=("section" if self._has_chapter else "subsection"),
                style=SimpleCounterStyle.Arabic,
            ),
        )
        setup.define_counter_rendering(
            "h3",
            SimpleCounterFormat(
                name=("subsection" if self._has_chapter else "subsubsection"),
                style=SimpleCounterStyle.Arabic,
            ),
        )
        setup.define_counter_rendering(
            "h4",
            SimpleCounterFormat(
                name=("subsubsection" if self._has_chapter else "subsubsubsection"),
                style=SimpleCounterStyle.Arabic,
            ),
        )
        setup.define_counter_rendering(
            "appendix",
            SimpleCounterFormat(
                name="appendix",
                style=SimpleCounterStyle.AlphUpper,
            ),
        )
        # TODO this shouldn't be necessary, there's a bug in the counter code
        setup.request_counter_parent("appendix", parent_counter=None)
        setup.request_counter_parent("h1", parent_counter=None)
        setup.request_counter_parent("h2", parent_counter="h1")
        setup.request_counter_parent("h3", parent_counter="h2")
        setup.request_counter_parent("h4", parent_counter="h3")

        # TODO emit pandoc yaml preamble for Markdown
        # TODO emit <html><head> blah for HTML?
        # TODO note that in HTML <title> doesn't accept formatting inside

    known_headers: List[Union[BasicHeader, AppendixHeader]]

    def _make_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]] | None:
        self.known_headers = []

        def visit_header(s: Union[BasicHeader, AppendixHeader]) -> None:
            self.known_headers.append(s)

        return [((BasicHeader, AppendixHeader), visit_header)]

    def _emit_title(
        self,
        title: TitleBlock,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        if title.metadata.title:
            renderer.emit(
                DocSegment(
                    BasicHeader(
                        title=Inlines([title.metadata.title]), anchor=None, weight=1
                    ),
                    contents=Blocks([]),
                    subsegments=[],
                )
            )
        if title.metadata.subtitle:
            renderer.emit(
                DocSegment(
                    BasicHeader(
                        title=Inlines([title.metadata.subtitle]),
                        anchor=None,
                        weight=2,
                    ),
                    contents=Blocks([]),
                    subsegments=[],
                )
            )
        if title.metadata.authors:
            renderer.emit(fmt.emph @ join_inlines(title.metadata.authors, Text(", ")))

    def _emit_toc(
        self,
        toc: TableOfContents,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        # TODO make lists a standard part of the MarkdownRenderer
        # TODO make lists able to be of Inline | Block so we don't need to wrap things in paragraph_of?
        renderer.emit(
            DocSegment(
                BasicHeader(title=Inlines([Text("Contents")]), anchor=None, weight=1),
                contents=Blocks(
                    [
                        fmt.itemize
                        @ [
                            fmt.item
                            @ Blocks(
                                [
                                    paragraph_of(
                                        Backref(
                                            id=header.anchor.id,
                                            kind=header.anchor.kind,
                                            label_contents=(
                                                fmt.bold
                                                @ [
                                                    renderer.anchor_to_number_text(
                                                        header.anchor
                                                    ),
                                                    Text(" \u2014 "),
                                                    header.title,
                                                ]
                                            ),
                                        )
                                    )
                                ]
                            )
                            for header in self.known_headers
                            if (header.weight <= toc.depth) and (header.anchor)
                        ]
                    ]
                ),
                subsegments=[],
            )
        )

    def _emit_structure(
        self,
        head: BasicHeader,
        contents: Blocks,
        subsegments: Iterator[DocSegment],
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        if renderer.in_html_mode:
            tag = f"h{head.weight}"

            with renderer.emit_tag(tag):
                if head.anchor:
                    renderer.emit(
                        head.anchor,
                        renderer.anchor_to_number_text(head.anchor),
                        Text(" \u2014 "),
                    )
                renderer.emit(head.title)
        else:
            renderer.emit_raw("#" * (head.weight) + " ")
            if head.anchor:
                renderer.emit(
                    head.anchor,
                    renderer.anchor_to_number_text(head.anchor),
                    Raw(" \u2014 "),
                )
            renderer.emit(head.title)

        renderer.emit_break_paragraph()
        renderer.emit_blockscope(contents)
        for s in subsegments:
            renderer.emit_segment(s)

    def _emit_appendix(
        self,
        head: AppendixHeader,
        contents: Blocks,
        subsegments: Iterator[DocSegment],
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        if renderer.in_html_mode:
            tag = f"h{head.weight}"

            with renderer.emit_tag(tag):
                renderer.emit(
                    head.anchor,
                    renderer.anchor_to_ref_text(head.anchor),
                    Text(" \u2014 "),
                    head.title,
                )
        else:
            renderer.emit_raw("#" * (head.weight) + " ")
            renderer.emit(
                head.anchor,
                renderer.anchor_to_ref_text(head.anchor),
                Text(" \u2014 "),
                head.title,
            )

        renderer.emit_break_paragraph()
        renderer.emit_blockscope(contents)
        for s in subsegments:
            renderer.emit_segment(s)
