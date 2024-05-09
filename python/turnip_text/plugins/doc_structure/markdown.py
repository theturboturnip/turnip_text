from typing import Iterator, Union

from turnip_text import BlockScope, DocSegment, Raw
from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.doc_structure import (
    AppendixHeader,
    StructureEnvPlugin,
    StructureHeader,
)
from turnip_text.render.manual_numbering import SimpleCounterFormat
from turnip_text.render.markdown.renderer import (
    MarkdownCounterStyle,
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
        setup.emitter.register_header(StructureHeader, self._emit_structure)
        setup.emitter.register_header(AppendixHeader, self._emit_appendix)
        setup.define_counter_rendering(
            "h1",
            SimpleCounterFormat(
                name=("chapter" if self._has_chapter else "section"),
                style=MarkdownCounterStyle.Arabic,
            ),
        )
        setup.define_counter_rendering(
            "h2",
            SimpleCounterFormat(
                name=("section" if self._has_chapter else "subsection"),
                style=MarkdownCounterStyle.Arabic,
            ),
        )
        setup.define_counter_rendering(
            "h3",
            SimpleCounterFormat(
                name=("subsection" if self._has_chapter else "subsubsection"),
                style=MarkdownCounterStyle.Arabic,
            ),
        )
        setup.define_counter_rendering(
            "h4",
            SimpleCounterFormat(
                name=("subsubsection" if self._has_chapter else "subsubsubsection"),
                style=MarkdownCounterStyle.Arabic,
            ),
        )
        setup.define_counter_rendering(
            "appendix",
            SimpleCounterFormat(
                name="appendix",
                style=MarkdownCounterStyle.AlphUpper,
            ),
        )
        # TODO this shouldn't be necessary, there's a bug in the counter code
        setup.request_counter_parent("appendix", parent_counter=None)
        setup.request_counter_parent("h1", parent_counter=None)
        setup.request_counter_parent("h2", parent_counter="h1")
        setup.request_counter_parent("h3", parent_counter="h2")
        setup.request_counter_parent("h4", parent_counter="h3")

    def _emit_structure(
        self,
        head: StructureHeader,
        contents: BlockScope,
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
                        Raw(" "),
                    )
                renderer.emit(head.title)
        else:
            renderer.emit_raw("#" * (head.weight) + " ")
            if head.anchor:
                renderer.emit(
                    head.anchor,
                    renderer.anchor_to_number_text(head.anchor),
                    Raw(" "),
                )
            renderer.emit(head.title)

        renderer.emit_break_paragraph()
        renderer.emit_blockscope(contents)
        for s in subsegments:
            renderer.emit_segment(s)

    def _emit_appendix(
        self,
        head: AppendixHeader,
        contents: BlockScope,
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
                    Raw(" \u2014 "),
                    head.title,
                )
        else:
            renderer.emit_raw("#" * (head.weight) + " ")
            renderer.emit(
                head.anchor,
                renderer.anchor_to_ref_text(head.anchor),
                Raw(" \u2014 "),
                head.title,
            )

        renderer.emit_break_paragraph()
        renderer.emit_blockscope(contents)
        for s in subsegments:
            renderer.emit_segment(s)
