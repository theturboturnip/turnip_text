from dataclasses import dataclass
from typing import Iterable, List, Optional, Sequence, Tuple, Union

from turnip_text import (
    Block,
    CoercibleToInline,
    Document,
    Header,
    Inline,
    Inlines,
    InlinesBuilder,
    Text,
    coerce_to_inline,
)
from turnip_text.doc.anchors import Anchor
from turnip_text.doc.user_nodes import UserNode
from turnip_text.env_plugins import DocEnv, EnvPlugin, FmtEnv, in_doc, pure_fmt
from turnip_text.helpers import UserInlineScopeBuilder
from typing_extensions import override


@dataclass
class BasicMetadata:
    title: Optional[Inline]
    subtitle: Optional[Inline]
    authors: List[Inline]
    # TODO
    # date: Optional[Inline]


@dataclass(frozen=True)
class TitleBlock(UserNode, Block):
    metadata: BasicMetadata
    anchor = None

    @override
    def child_nodes(self) -> Iterable[Block | Inline] | None:
        child_nodes = []
        if self.metadata.title:
            child_nodes.append(self.metadata.title)
        if self.metadata.subtitle:
            child_nodes.append(self.metadata.subtitle)
        for author in self.metadata.authors:
            child_nodes.append(author)
        return child_nodes


@dataclass(frozen=True)
class BasicHeader(UserNode, Header):
    title: Inlines  # The title of the segment
    anchor: Anchor | None
    """Set to None if the header as a whole is unnumbered.
    Has a non-None Anchor with `id == None` if the header is numbered, but didn't have a label."""
    weight: int

    @override
    def child_nodes(self) -> Inlines:
        return self.title


@dataclass(frozen=True)
class AppendixHeader(UserNode, Header):
    title: Inlines  # The title of the segment
    anchor: Anchor
    weight: int

    @override
    def child_nodes(self) -> Inlines:
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

    def build_from_inlines(self, inlines: Inlines) -> Header:
        if self.appendix:
            kind = "appendix"
        else:
            kind = f"h{self.weight}"
        weight = self.weight

        ty = AppendixHeader if self.appendix else BasicHeader

        if self.num:
            return ty(
                title=inlines,
                anchor=self.doc_env.register_new_anchor(kind, self.label),
                weight=weight,
            )  # type: ignore
        return ty(title=inlines, anchor=None, weight=weight)  # type: ignore


class StructureEnvPlugin(EnvPlugin):
    _metadata: Optional[BasicMetadata] = None
    """At most one BasicMetadata object exists for each StructureEnvPlugin"""

    def _doc_nodes(
        self,
    ) -> Sequence[type[Block] | type[Inline] | type[Header]]:
        return (
            BasicHeader,
            TableOfContents,
            TitleBlock,
        )

    # TODO an option to not do this?
    def _mutate_document(self, doc_env: DocEnv, fmt: FmtEnv, doc: Document) -> None:
        super()._mutate_document(doc_env, fmt, doc)
        # TODO better DFS walking
        if not any(isinstance(b, TableOfContents) for b in doc.contents):
            doc.contents.insert_block(0, self.toc())
        if self._metadata and not any(isinstance(b, TitleBlock) for b in doc.contents):
            doc.contents.insert_block(0, self.title_block())

    def _set_metadata(
        self,
        /,
        title: Optional[CoercibleToInline] = None,
        subtitle: Optional[CoercibleToInline] = None,
        authors: Optional[Sequence[CoercibleToInline]] = None,
    ) -> BasicMetadata:
        """If self.metadata hasn't already been set, set it to a new BasicMetadata object with the given arguments.
        Otherwise raise a ValueError()"""
        if self._metadata:
            raise ValueError("Cannot set document metadata twice")
        self._metadata = BasicMetadata(
            title=coerce_to_inline(title) if title else None,
            subtitle=coerce_to_inline(subtitle) if subtitle else None,
            authors=[coerce_to_inline(author) for author in authors] if authors else [],
        )
        return self._metadata

    @in_doc
    def set_metadata(
        self,
        doc_env: DocEnv,
        /,
        title: Optional[CoercibleToInline] = None,
        subtitle: Optional[CoercibleToInline] = None,
        authors: Optional[Sequence[CoercibleToInline]] = None,
    ) -> None:
        if (title is None) and (subtitle is None) and (authors is None):
            return
        self._set_metadata(
            title=title,
            subtitle=subtitle,
            authors=authors,
        )

    @in_doc
    def title_block(
        self,
        doc_env: DocEnv,
        /,
        title: Optional[CoercibleToInline] = None,
        subtitle: Optional[CoercibleToInline] = None,
        authors: Optional[Sequence[CoercibleToInline]] = None,
    ) -> TitleBlock:
        if (title is None) and (subtitle is None) and (authors is None):
            if not self._metadata:
                raise RuntimeError(
                    "Cannot create a title_block() with no metadata without first calling set_metadata()"
                )
            return TitleBlock(self._metadata)
        else:
            return TitleBlock(
                self._set_metadata(title=title, subtitle=subtitle, authors=authors)
            )

    @in_doc
    def h(
        self,
        doc_env: DocEnv,
        weight: int,
        label: Optional[str] = None,
        num: bool = True,
    ) -> InlinesBuilder:
        return StructureHeaderGenerator(doc_env, weight, label, num)

    @in_doc
    def h1(
        self, doc_env: DocEnv, label: Optional[str] = None, num: bool = True
    ) -> InlinesBuilder:
        return self.h(1, label, num)

    @in_doc
    def h2(
        self, doc_env: DocEnv, label: Optional[str] = None, num: bool = True
    ) -> InlinesBuilder:
        return self.h(2, label, num)

    @in_doc
    def h3(
        self, doc_env: DocEnv, label: Optional[str] = None, num: bool = True
    ) -> InlinesBuilder:
        return self.h(3, label, num)

    @in_doc
    def h4(
        self, doc_env: DocEnv, label: Optional[str] = None, num: bool = True
    ) -> InlinesBuilder:
        return self.h(4, label, num)

    @in_doc
    def appendix(self, doc_env: DocEnv, label: Optional[str] = None) -> InlinesBuilder:
        """Builds an inline scope to create a header that starts an appendix at weight=1."""
        return StructureHeaderGenerator(
            doc_env, weight=1, label=label, num=True, appendix=True
        )

    @pure_fmt
    def toc(self, fmt: FmtEnv, depth: int = 3) -> TableOfContents:
        return TableOfContents(depth)
