from dataclasses import dataclass
from typing import Iterable, Optional, Sequence, Set

from turnip_text import Block, Header, Inline, Inlines, InlinesBuilder
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.doc.user_nodes import NodePortal, UserNode
from turnip_text.env_plugins import DocEnv, EnvPlugin, FmtEnv, in_doc, pure_fmt
from turnip_text.helpers import inlines_builder
from typing_extensions import override


@dataclass(frozen=True)
class FootnoteRef(UserNode, Inline, NodePortal):
    portal_to: Backref
    anchor = None

    @override
    def child_nodes(self) -> Iterable[Block | Inline] | None:
        return None


@dataclass(frozen=True)
class FootnoteContents(UserNode, Block):
    anchor: Anchor
    contents: Inline

    @override
    def child_nodes(self) -> Iterable[Block | Inline] | None:
        return (self.contents,)


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
    def footnote(self, doc_env: DocEnv) -> InlinesBuilder:
        @inlines_builder
        def footnote_builder(inlines: Inlines) -> Inline:
            anchor = doc_env.register_new_anchor_with_float(
                "footnote", None, lambda anchor: FootnoteContents(anchor, inlines)
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
    def footnote_text(self, doc_env: DocEnv, footnote_id: str) -> InlinesBuilder:
        # Store the contents of a block scope and associate them with a specific footnote label
        @inlines_builder
        def handle_block_contents(contents: Inlines) -> Optional[Block]:
            doc_env.register_new_anchor_with_float(
                "footnote",
                footnote_id,
                lambda anchor: FootnoteContents(anchor, contents),
            )
            return None

        return handle_block_contents
