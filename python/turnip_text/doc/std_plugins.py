from dataclasses import dataclass
from enum import Enum
from typing import (
    Any,
    Callable,
    Dict,
    Iterable,
    List,
    Optional,
    Sequence,
    Set,
    Tuple,
    Type,
    Union,
    cast,
)

from turnip_text import (
    Block,
    BlockScope,
    BlockScopeBuilder,
    DocSegment,
    Document,
    Header,
    Inline,
    InlineScope,
    InlineScopeBuilder,
    Paragraph,
    Sentence,
    Text,
    TurnipTextSource,
)
from turnip_text.doc import DocPlugin, DocState, FormatContext, stateful, stateless
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.doc.user_nodes import (
    NodePortal,
    UserAnchorBlock,
    UserAnchorHeader,
    UserBlock,
    UserInline,
    VisitableNode,
)
from turnip_text.helpers import block_scope_builder, inline_scope_builder, paragraph_of


def STD_DOC_PLUGINS(allow_multiple_footnote_refs: bool = False) -> List[DocPlugin]:
    return [
        StructureDocPlugin(),
        CitationDocPlugin(),
        FootnoteDocPlugin(allow_multiple_refs=allow_multiple_footnote_refs),
        ListDocPlugin(),
        InlineFormatDocPlugin(),
        UrlDocPlugin(),
        SubfileDocPlugin(),
    ]


@dataclass(frozen=True)
class FootnoteRef(Inline, NodePortal):
    portal_to: Backref


@dataclass(frozen=True)
class FootnoteContents(Block):
    anchor: Anchor
    contents: Inline


@dataclass(frozen=True)
class HeadedBlock(UserAnchorBlock):
    contents: Tuple[Inline, BlockScope]
    num: bool = True


# Moons ago I considered replacing this with Backref. This should not be replaced with Backref,
# because it has specific context in how it is rendered. Renderer plugins may mutate the document
# and replace these with Backrefs if they so choose.
@dataclass(frozen=True)
class Citation(UserInline):
    contents: InlineScope | None  # the citation note
    citekeys: Set[str]

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        return Citation(citekeys=self.citekeys, contents=inls)


@dataclass(frozen=True)
class CiteAuthor(Inline):
    citekey: str


class Bibliography(Block):
    pass


@dataclass(frozen=True)
class NamedUrl(UserInline, InlineScopeBuilder):
    contents: Iterable[Inline] | None
    url: str

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        return NamedUrl(url=self.url, contents=inls)


class DisplayListType(Enum):
    Enumerate = 0
    Itemize = 1


@dataclass(frozen=True)
class DisplayList(UserBlock):
    contents: List[Union["DisplayList", "DisplayListItem"]]
    list_type: DisplayListType  # TODO could reuse Numbering from render.counters?


@dataclass(frozen=True)
class DisplayListItem(UserBlock):
    contents: BlockScope


# TODO strikethrough? sub/superscript?
class InlineFormattingType(Enum):
    Italic = 0
    Bold = 1
    Underline = 2
    Emph = 3  # Usually italic
    Strong = 4  # Usually bold
    SingleQuote = 5
    DoubleQuote = 6


@dataclass(frozen=True)
class InlineFormatted(UserInline):
    contents: InlineScope
    format_type: InlineFormattingType


@dataclass(frozen=True)
class StructureBlockHeader(UserAnchorHeader):
    contents: BlockScope  # The title of the segment (TODO once the interpreter allows it, make this use InlineScope. See _headingn)
    anchor: Optional[
        Anchor
    ]  # May be None if this DocSegment is unnumbered. Otherwise necessary so it can be counted, but the ID may be None
    weight: int


@dataclass(frozen=True)
class TableOfContents(Block):
    pass


# TODO make this a InlineScopeBuilder. Right now an InlineScopeBuilder can't return Header,
# because once you're parsing inline content you're in "inline mode".
class StructureBlockHeaderGenerator(BlockScopeBuilder):
    doc: DocState
    weight: int
    label: Optional[str]
    num: bool

    def __init__(
        self, doc: DocState, weight: int, label: Optional[str], num: bool
    ) -> None:
        super().__init__()
        self.doc = doc
        self.weight = weight
        self.label = label
        self.num = num

    def __call__(
        self, label: Optional[str] = None, num: bool = True
    ) -> "StructureBlockHeaderGenerator":
        return StructureBlockHeaderGenerator(self.doc, self.weight, label, num)

    def build_from_blocks(self, bs: BlockScope) -> StructureBlockHeader:
        kind = f"h{self.weight}"
        weight = self.weight

        if self.num:
            return StructureBlockHeader(
                contents=bs,
                anchor=self.doc.anchors.register_new_anchor(kind, self.label),
                weight=weight,
            )
        return StructureBlockHeader(contents=bs, anchor=None, weight=weight)


class StructureDocPlugin(DocPlugin):
    def _doc_nodes(
        self,
    ) -> Sequence[type[Block] | type[Inline] | type[Header]]:
        return (
            StructureBlockHeader,
            # TableOfContents, # TODO
        )

    @stateful
    def heading1(
        self, state: DocState, label: Optional[str] = None, num: bool = True
    ) -> BlockScopeBuilder:
        return StructureBlockHeaderGenerator(state, 1, label, num)

    @stateful
    def heading2(
        self, state: DocState, label: Optional[str] = None, num: bool = True
    ) -> BlockScopeBuilder:
        return StructureBlockHeaderGenerator(state, 2, label, num)

    @stateful
    def heading3(
        self, state: DocState, label: Optional[str] = None, num: bool = True
    ) -> BlockScopeBuilder:
        return StructureBlockHeaderGenerator(state, 3, label, num)

    @stateful
    def heading4(
        self, state: DocState, label: Optional[str] = None, num: bool = True
    ) -> BlockScopeBuilder:
        return StructureBlockHeaderGenerator(state, 4, label, num)

    # TODO
    # @stateless
    # def toc(self, fmt: FormatContext) -> TableOfContents:
    #     return TableOfContents()


class CitationDocPlugin(DocPlugin):
    _has_citations: bool = False
    _has_bib: bool = False

    def _doc_nodes(
        self,
    ) -> Sequence[type[Block] | type[Inline] | type[Header]]:
        return (
            Citation,
            CiteAuthor,
            Bibliography,
        )

    def _mutate_document(
        self, doc: DocState, fmt: FormatContext, toplevel: Document
    ) -> Document:
        if not self._has_bib:
            toplevel.push_segment(
                DocSegment(
                    doc.heading1(num=False) @ paragraph_of("Bibliography"),
                    BlockScope([Bibliography()]),
                    [],
                )
            )
        return toplevel

    @stateful
    def cite(self, doc: DocState, *citekeys: str) -> Inline:
        citekey_set: set[str] = set(citekeys)
        for c in citekey_set:
            if not isinstance(c, str):
                raise ValueError(f"Inappropriate citation key: {c}. Must be a string")
        self._has_citations = True
        return Citation(citekeys=citekey_set, contents=None)

    @stateless
    def citeauthor(self, fmt: FormatContext, citekey: str) -> Inline:
        return CiteAuthor(citekey)

    @stateful
    def bibliography(self, doc: DocState) -> Bibliography:
        self._has_bib = True
        return Bibliography()


class FootnoteDocPlugin(DocPlugin):
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

    @stateful
    def footnote(self, doc: DocState) -> InlineScopeBuilder:
        @inline_scope_builder
        def footnote_builder(contents: InlineScope) -> Inline:
            anchor = doc.anchors.register_new_anchor_with_float(
                "footnote", None, lambda anchor: FootnoteContents(anchor, contents)
            )
            self.footnotes_with_refs.add(anchor.id)
            return FootnoteRef(portal_to=anchor.to_backref())

        return footnote_builder

    @stateless
    def footnote_ref(self, fmt: FormatContext, footnote_id: str) -> Inline:
        if (not self.allow_multiple_refs) and (footnote_id in self.footnotes_with_refs):
            raise ValueError(f"Tried to refer to footnote {footnote_id} twice!")
        self.footnotes_with_refs.add(footnote_id)
        return FootnoteRef(
            portal_to=Backref(id=footnote_id, kind="footnote", label_contents=None)
        )

    @stateful
    def footnote_text(self, doc: DocState, footnote_id: str) -> BlockScopeBuilder:
        # Store the contents of a block scope and associate them with a specific footnote label
        # TODO ah hell make InlineScopeBuilder able to return None so we can use it here
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Optional[Block]:
            p = next(iter(contents))
            assert isinstance(p, Paragraph)
            doc.anchors.register_new_anchor_with_float(
                "footnote",
                footnote_id,
                lambda anchor: FootnoteContents(
                    anchor, InlineScope(list(next(iter(p))))
                ),
            )
            return None

        return handle_block_contents


class InlineFormatDocPlugin(DocPlugin):
    def _doc_nodes(self) -> Sequence[type[Block] | type[Inline]]:
        return (InlineFormatted,)

    @inline_scope_builder
    @staticmethod
    def italic(items: InlineScope) -> Inline:
        return InlineFormatted(contents=items, format_type=InlineFormattingType.Italic)

    @inline_scope_builder
    @staticmethod
    def bold(items: InlineScope) -> Inline:
        return InlineFormatted(contents=items, format_type=InlineFormattingType.Bold)

    @inline_scope_builder
    @staticmethod
    def underline(items: InlineScope) -> Inline:
        return InlineFormatted(
            contents=items, format_type=InlineFormattingType.Underline
        )

    @inline_scope_builder
    @staticmethod
    def emph(items: InlineScope) -> Inline:
        return InlineFormatted(contents=items, format_type=InlineFormattingType.Emph)

    @inline_scope_builder
    @staticmethod
    def strong(items: InlineScope) -> Inline:
        return InlineFormatted(contents=items, format_type=InlineFormattingType.Strong)

    @inline_scope_builder
    @staticmethod
    def squote(items: InlineScope) -> Inline:
        return InlineFormatted(
            contents=items, format_type=InlineFormattingType.SingleQuote
        )

    @inline_scope_builder
    @staticmethod
    def enquote(items: InlineScope) -> Inline:
        return InlineFormatted(
            contents=items, format_type=InlineFormattingType.DoubleQuote
        )


class ListDocPlugin(DocPlugin):
    def _doc_nodes(self) -> Sequence[type[Block] | type[Inline]]:
        return (DisplayList, DisplayListItem)

    @block_scope_builder
    @staticmethod
    def enumerate(contents: BlockScope) -> Block:
        items = list(contents)
        if not all(isinstance(x, (DisplayListItem, DisplayList)) for x in items):
            raise TypeError(
                f"Found blocks in this list that were not list [item]s or other lists!"
            )
        return DisplayList(
            list_type=DisplayListType.Enumerate,
            contents=cast(List[DisplayListItem | DisplayList], items),
        )

    @block_scope_builder
    @staticmethod
    def itemize(contents: BlockScope) -> Block:
        items = list(contents)
        if not all(isinstance(x, (DisplayListItem, DisplayList)) for x in items):
            raise TypeError(
                f"Found blocks in this list that were not list [item]s or other lists!"
            )
        return DisplayList(
            list_type=DisplayListType.Itemize,
            contents=cast(List[DisplayListItem | DisplayList], items),
        )

    @block_scope_builder
    @staticmethod
    def item(block_scope: BlockScope) -> Block:
        return DisplayListItem(contents=block_scope)


class UrlDocPlugin(DocPlugin):
    def _doc_nodes(self) -> Sequence[type[Block] | type[Inline]]:
        return (NamedUrl,)

    @stateless
    def url(self, fmt: FormatContext, url: str, name: Optional[str] = None) -> Inline:
        if not isinstance(url, str):
            raise ValueError(f"Url {url} must be a string")
        if name is not None and not isinstance(name, str):
            raise ValueError(f"Url name {name} must be a string if not None")
        return NamedUrl(
            contents=(Text(name),) if name is not None else None,
            url=url,
        )


class SubfileDocPlugin(DocPlugin):
    @stateful
    def subfile(self, doc: DocState, project_relative_path: str) -> TurnipTextSource:
        return doc.build_sys.resolve_turnip_text_source(project_relative_path)
