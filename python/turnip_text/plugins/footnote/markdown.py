from typing import List, Sequence, Tuple

from turnip_text import Block, BlockScope, DocSegment, Document, Header, Inline, Text
from turnip_text.build_system import BuildSystem
from turnip_text.doc.anchors import Backref
from turnip_text.doc.dfs import VisitorFilter, VisitorFunc
from turnip_text.env_plugins import DocEnv, FmtEnv
from turnip_text.plugins.footnote import (
    FootnoteContents,
    FootnoteEnvPlugin,
    FootnoteRef,
)
from turnip_text.render.manual_numbering import SimpleCounterFormat
from turnip_text.render.markdown.renderer import (
    MarkdownCounterStyle,
    MarkdownPlugin,
    MarkdownRenderer,
    MarkdownSetup,
)


class FootnoteList(Block):
    pass


# TODO FootnoteBeforeNextParagraphRenderPlugin
# - FootnoteAfterBlock may try to emit something in the middle of a custom block, Paragraphs are (I think?) guaranteed to be inside a BlockScope and we can kind emit them there
# TODO this is effectively an alternate/nonstandard implementation - move it out, we haven't agreed on a standard footntoe plugin
class MarkdownFootnotePlugin_AtEnd(MarkdownPlugin, FootnoteEnvPlugin):
    footnote_anchors: List[Backref]

    def __init__(self) -> None:
        super().__init__()
        self.footnote_anchors = []

    def _doc_nodes(
        self,
    ) -> Sequence[type[Block] | type[Inline] | type[Header]]:
        return [FootnoteList] + list(super()._doc_nodes())

    def _mutate_document(
        self, doc_env: DocEnv, fmt: FmtEnv, toplevel: Document
    ) -> Document:
        toplevel = super()._mutate_document(doc_env, fmt, toplevel)
        toplevel.push_segment(
            DocSegment(
                doc_env.heading1(num=False) @ "Footnotes",
                BlockScope([FootnoteList()]),
                [],
            )
        )
        return toplevel

    def _register(self, build_sys: BuildSystem, setup: MarkdownSetup) -> None:
        setup.emitter.register_block_or_inline(FootnoteRef, self._emit_footnote_ref)
        setup.emitter.register_block_or_inline(
            FootnoteContents, lambda _, __, ___: None
        )
        setup.emitter.register_block_or_inline(FootnoteList, self._emit_footnotes)
        setup.define_counter_rendering(
            "footnote",
            SimpleCounterFormat(name="^", style=MarkdownCounterStyle.Arabic),
        )

    def _make_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]] | None:
        return [(FootnoteRef, lambda f: self.footnote_anchors.append(f.portal_to))]

    def _emit_footnote_ref(
        self,
        footnote: FootnoteRef,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        renderer.emit(footnote.portal_to)

    def _emit_footnotes(
        self,
        footnotes: FootnoteList,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        for backref in self.footnote_anchors:
            anchor, footnote = renderer.anchors.lookup_backref_float(backref)
            assert isinstance(footnote, FootnoteContents)
            renderer.emit(
                anchor,
                renderer.anchor_to_ref_text(anchor),
                Text(f": "),
                footnote.contents,
            )
            renderer.emit_break_sentence()
        renderer.emit_break_paragraph()
