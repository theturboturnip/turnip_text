from dataclasses import dataclass
from typing import Optional, Sequence, Set

from turnip_text import Block, Header, Inline, InlineScope, InlineScopeBuilder
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.doc.user_nodes import NodePortal
from turnip_text.env_plugins import DocEnv, EnvPlugin, FmtEnv, in_doc, pure_fmt
from turnip_text.helpers import inline_scope_builder


@dataclass(frozen=True)
class FootnoteRef(Inline, NodePortal):
    portal_to: Backref


@dataclass(frozen=True)
class FootnoteContents(Block):
    anchor: Anchor
    contents: Inline


class FootnoteEnvPlugin(EnvPlugin):
    allow_multiple_refs: bool
    footnotes_with_refs: Set[str]

    def __init__(self, allow_multiple_refs: bool = False) -> None:
        super().__init__()
        self.allow_multiple_refs = allow_multiple_refs
        self.footnotes_with_refs = set()

    def _doc_nodes(
        self,
    ) -> Sequence[type[Block] | type[Inline] | type[Header]]:
        return (FootnoteRef, FootnoteContents)

    def _countables(self) -> Sequence[str]:
        return ("footnote",)

    @in_doc
    def footnote(self, doc_env: DocEnv) -> InlineScopeBuilder:
        @inline_scope_builder
        def footnote_builder(contents: InlineScope) -> Inline:
            anchor = doc_env.register_new_anchor_with_float(
                "footnote", None, lambda anchor: FootnoteContents(anchor, contents)
            )
            self.footnotes_with_refs.add(anchor.id)
            return FootnoteRef(portal_to=anchor.to_backref())

        return footnote_builder

    @pure_fmt
    def footnote_ref(self, fmt: FmtEnv, footnote_id: str) -> Inline:
        if (not self.allow_multiple_refs) and (footnote_id in self.footnotes_with_refs):
            raise ValueError(f"Tried to refer to footnote {footnote_id} twice!")
        self.footnotes_with_refs.add(footnote_id)
        return FootnoteRef(
            portal_to=Backref(id=footnote_id, kind="footnote", label_contents=None)
        )

    @in_doc
    def footnote_text(self, doc_env: DocEnv, footnote_id: str) -> InlineScopeBuilder:
        # Store the contents of a block scope and associate them with a specific footnote label
        @inline_scope_builder
        def handle_block_contents(contents: InlineScope) -> Optional[Block]:
            doc_env.register_new_anchor_with_float(
                "footnote",
                footnote_id,
                lambda anchor: FootnoteContents(anchor, contents),
            )
            return None

        return handle_block_contents
