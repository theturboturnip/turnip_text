import uuid
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
    DocSegmentHeader,
    Inline,
    InlineScope,
    InlineScopeBuilder,
    Paragraph,
    Sentence,
    UnescapedText,
)
from turnip_text.doc import DocPlugin, DocState, FormatContext, stateful, stateless
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.doc.user_nodes import (
    UserAnchorBlock,
    UserAnchorDocSegmentHeader,
    UserBlock,
    UserInline,
    VisitableNode,
)
from turnip_text.helpers import block_scope_builder, inline_scope_builder


def STD_DOC_PLUGINS() -> List[DocPlugin]:
    return [
        StructureDocPlugin(),
        CitationDocPlugin(),
        FootnoteDocPlugin(),
        ListDocPlugin(),
        InlineFormatDocPlugin(),
        UrlDocPlugin(),
    ]

@dataclass(frozen=True)
class FootnoteRef(Inline):
    ref: Backref


@dataclass(frozen=True)
class HeadedBlock(UserAnchorBlock):
    contents: Tuple[Inline, BlockScope]
    num: bool = True


@dataclass(frozen=True)
class Citation(UserInline):
    contents: InlineScope | None  # the citation note
    citekeys: Set[str]

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        return Citation(citekeys=self.citekeys, contents=inls)


@dataclass(frozen=True)
class CiteAuthor(Inline):
    citekey: str


class Bibliography(DocSegmentHeader):
    weight = 0


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
class StructureBlockHeader(UserAnchorDocSegmentHeader):
    contents: BlockScope # The title of the segment (TODO once the interpreter allows it, make this use InlineScope. See _headingn)
    anchor: Optional[Anchor] # May be None if this DocSegment is unnumbered. TODO otherwise necessary because it's needed for counters??? argh.
    weight: int


@dataclass(frozen=True)
class TableOfContents(Block):
    pass


# TODO make the headings builders that are also callable?
class StructureDocPlugin(DocPlugin):
    def _doc_nodes(self) -> Sequence[type[Block] | type[Inline] | type[DocSegmentHeader]]:
        return (
            StructureBlockHeader,
            # TableOfContents, # TODO
        )

    # TODO make this return InlineScopeBuilder. Right now an InlineScopeBuilder can't return DocSegmentHeader, 
    # because once you're parsing inline content you're in "inline mode".
    def _headingn(self, state: DocState, label: str, num: bool, n: int) -> BlockScopeBuilder:
        kind = f"h{n}"
        weight = n
        @block_scope_builder
        def builder(bs: BlockScope) -> StructureBlockHeader:
            if num:
                return StructureBlockHeader(contents=bs, anchor=state.anchors.register_new_anchor(kind, label), weight=weight)
            return StructureBlockHeader(contents=bs, anchor=None, weight=weight)
        return builder

    @stateful
    def heading1(self, state: DocState, label: str, num: bool = True) -> BlockScopeBuilder:
        return self._headingn(state, label, num, 1)

    @stateful
    def heading2(self, state: DocState, label: str, num: bool = True) -> BlockScopeBuilder:
        return self._headingn(state, label, num, 2)

    @stateful
    def heading3(self, state: DocState, label: str, num: bool = True) -> BlockScopeBuilder:
        return self._headingn(state, label, num, 3)

    @stateful
    def heading4(self, state: DocState, label: str, num: bool = True) -> BlockScopeBuilder:
        return self._headingn(state, label, num, 4)
    
    # TODO
    # @stateless
    # def toc(self, fmt: FormatContext) -> TableOfContents:
    #     return TableOfContents()


class CitationDocPlugin(DocPlugin):
    _has_citations: bool = False
    _has_bib: bool = False

    def _doc_nodes(
        self,
    ) -> Sequence[type[Block] | type[Inline] | type[DocSegmentHeader]]:
        return (
            Citation,
            CiteAuthor,
            Bibliography,
        )

    def _mutate_document(
        self, doc: DocState, fmt: FormatContext, toplevel: DocSegment
    ) -> DocSegment:
        if not self._has_bib:
            toplevel.push_subsegment(DocSegment(Bibliography(), BlockScope(), []))
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

    @property
    @stateful
    def bibliography(self, doc: DocState) -> DocSegmentHeader:
        self._has_bib = True
        return Bibliography()


class FootnoteDocPlugin(DocPlugin):
    def _doc_nodes(
        self,
    ) -> Sequence[type[Block] | type[Inline] | type[DocSegmentHeader]]:
        return (
            FootnoteRef,
        )

    def _countables(self) -> Sequence[str]:
        return ("footnote",)

    @property
    @stateful
    def footnote(self, doc: DocState) -> InlineScopeBuilder:
        @inline_scope_builder
        def footnote_builder(contents: InlineScope) -> Inline:
            footnote_id = str(uuid.uuid4())
            anchor = doc.anchors.register_new_anchor("footnote", footnote_id)
            doc.add_float(anchor, Paragraph([Sentence([contents])]))
            return FootnoteRef(anchor.to_backref())

        return footnote_builder

    @stateless
    def footnote_ref(self, fmt: FormatContext, footnote_id: str) -> Inline:
        # TODO make it only possible to have a single footnoteref per footnote?
        return FootnoteRef(
            Backref(id=footnote_id, kind="footnote", label_contents=None)
        )

    @stateful
    def footnote_text(self, doc: DocState, footnote_id: str) -> BlockScopeBuilder:
        # Store the contents of a block scope and associate them with a specific footnote label
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Optional[Block]:
            anchor = doc.anchors.register_new_anchor("footnote", footnote_id)
            doc.add_float(anchor, contents)
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
            contents=(UnescapedText(name),) if name is not None else None,
            url=url,
        )
