from dataclasses import dataclass
from typing import Optional, Sequence, Union

from typing_extensions import override

from turnip_text import Block, Header, Inline, InlineScope, InlineScopeBuilder
from turnip_text.doc.anchors import Anchor
from turnip_text.doc.user_nodes import UserNode
from turnip_text.env_plugins import DocEnv, EnvPlugin, FmtEnv, in_doc, pure_fmt
from turnip_text.helpers import UserInlineScopeBuilder


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
class AppendixHeader(UserNode, Header):
    title: InlineScope  # The title of the segment
    anchor: Anchor
    weight: int

    @override
    def child_nodes(self) -> InlineScope:
        return self.title


@dataclass(frozen=True)
class TableOfContents(Block):
    depth: int


class StructureHeaderGenerator(UserInlineScopeBuilder):
    doc_env: DocEnv
    weight: int
    label: Optional[str]
    num: bool
    appendix: bool

    def __init__(
        self,
        doc_env: DocEnv,
        weight: int,
        label: Optional[str],
        num: bool,
        appendix: bool = False,
    ) -> None:
        super().__init__()
        self.doc_env = doc_env
        self.weight = weight
        self.label = label
        self.num = num
        self.appendix = appendix

    def __call__(
        self, label: Optional[str] = None, num: bool = True
    ) -> "StructureHeaderGenerator":
        return StructureHeaderGenerator(
            self.doc_env, self.weight, label, num, self.appendix
        )

    def build_from_inlines(self, inlines: InlineScope) -> Header:
        if self.appendix:
            kind = "appendix"
        else:
            kind = f"h{self.weight}"
        weight = self.weight

        ty = AppendixHeader if self.appendix else StructureHeader

        if self.num:
            return ty(
                title=inlines,
                anchor=self.doc_env.register_new_anchor(kind, self.label),
                weight=weight,
            )  # type: ignore
        return ty(title=inlines, anchor=None, weight=weight)  # type: ignore


class StructureEnvPlugin(EnvPlugin):
    def _doc_nodes(
        self,
    ) -> Sequence[type[Block] | type[Inline] | type[Header]]:
        return (
            StructureHeader,
            TableOfContents,
        )

    @in_doc
    def h(
        self,
        doc_env: DocEnv,
        weight: int,
        label: Optional[str] = None,
        num: bool = True,
    ) -> InlineScopeBuilder:
        return StructureHeaderGenerator(doc_env, weight, label, num)

    @in_doc
    def h1(
        self, doc_env: DocEnv, label: Optional[str] = None, num: bool = True
    ) -> InlineScopeBuilder:
        return self.h(1, label, num)

    @in_doc
    def h2(
        self, doc_env: DocEnv, label: Optional[str] = None, num: bool = True
    ) -> InlineScopeBuilder:
        return self.h(2, label, num)

    @in_doc
    def h3(
        self, doc_env: DocEnv, label: Optional[str] = None, num: bool = True
    ) -> InlineScopeBuilder:
        return self.h(3, label, num)

    @in_doc
    def h4(
        self, doc_env: DocEnv, label: Optional[str] = None, num: bool = True
    ) -> InlineScopeBuilder:
        return self.h(4, label, num)

    @in_doc
    def appendix(
        self, doc_env: DocEnv, label: Optional[str] = None
    ) -> InlineScopeBuilder:
        """Builds an inline scope to create a header that starts an appendix at weight=1."""
        return StructureHeaderGenerator(
            doc_env, weight=1, label=label, num=True, appendix=True
        )

    @pure_fmt
    def toc(self, fmt: FmtEnv, depth: int = 3) -> TableOfContents:
        return TableOfContents(depth)
