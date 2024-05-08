from dataclasses import dataclass
from typing import Optional, Sequence

from typing_extensions import override

from turnip_text import Block, Header, Inline, InlineScope, InlineScopeBuilder
from turnip_text.doc.anchors import Anchor
from turnip_text.doc.user_nodes import UserNode
from turnip_text.env_plugins import DocEnv, EnvPlugin, in_doc


@dataclass(frozen=True)
class StructureHeader(UserNode, Header):
    title: InlineScope  # The title of the segment
    anchor: Anchor | None
    """Set to None if the header as a whole is unnumbered.
    Has a non-None Anchor with `id == None` if the header is numbered, but didn't have a label."""
    weight: int

    @override
    def child_nodes(self) -> InlineScope:
        return self.title


@dataclass(frozen=True)
class TableOfContents(Block):
    pass


class StructureHeaderGenerator(InlineScopeBuilder):
    doc_env: DocEnv
    weight: int
    label: Optional[str]
    num: bool

    def __init__(
        self, doc_env: DocEnv, weight: int, label: Optional[str], num: bool
    ) -> None:
        super().__init__()
        self.doc_env = doc_env
        self.weight = weight
        self.label = label
        self.num = num

    def __call__(
        self, label: Optional[str] = None, num: bool = True
    ) -> "StructureHeaderGenerator":
        return StructureHeaderGenerator(self.doc_env, self.weight, label, num)

    def build_from_inlines(self, inlines: InlineScope) -> StructureHeader:
        kind = f"h{self.weight}"
        weight = self.weight

        if self.num:
            return StructureHeader(
                title=inlines,
                anchor=self.doc_env.register_new_anchor(kind, self.label),
                weight=weight,
            )
        return StructureHeader(title=inlines, anchor=None, weight=weight)


class StructureEnvPlugin(EnvPlugin):
    def _doc_nodes(
        self,
    ) -> Sequence[type[Block] | type[Inline] | type[Header]]:
        return (
            StructureHeader,
            # TableOfContents, # TODO
        )

    @in_doc
    def heading1(
        self, doc_env: DocEnv, label: Optional[str] = None, num: bool = True
    ) -> InlineScopeBuilder:
        return StructureHeaderGenerator(doc_env, 1, label, num)

    @in_doc
    def heading2(
        self, doc_env: DocEnv, label: Optional[str] = None, num: bool = True
    ) -> InlineScopeBuilder:
        return StructureHeaderGenerator(doc_env, 2, label, num)

    @in_doc
    def heading3(
        self, doc_env: DocEnv, label: Optional[str] = None, num: bool = True
    ) -> InlineScopeBuilder:
        return StructureHeaderGenerator(doc_env, 3, label, num)

    @in_doc
    def heading4(
        self, doc_env: DocEnv, label: Optional[str] = None, num: bool = True
    ) -> InlineScopeBuilder:
        return StructureHeaderGenerator(doc_env, 4, label, num)

    # TODO
    # @pure_fmt
    # def toc(self, fmt: FmtEnv) -> TableOfContents:
    #     return TableOfContents()
