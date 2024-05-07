import re
from collections import defaultdict
from dataclasses import dataclass
from enum import Enum
from typing import (
    Callable,
    Dict,
    Iterable,
    List,
    Optional,
    Sequence,
    Set,
    Tuple,
    Union,
    cast,
)

from typing_extensions import override

from turnip_text import (
    Block,
    BlockScope,
    DocSegment,
    Document,
    Header,
    Inline,
    InlineScope,
    InlineScopeBuilder,
    Text,
    TurnipTextSource,
)
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.doc.user_nodes import NodePortal, UserNode
from turnip_text.env_plugins import DocEnv, EnvPlugin, FmtEnv, in_doc, pure_fmt
from turnip_text.helpers import block_scope_builder, inline_scope_builder


def STD_DOC_PLUGINS(allow_multiple_footnote_refs: bool = False) -> List[EnvPlugin]:
    return [
        StructureEnvPlugin(),
        CitationEnvPlugin(),
        FootnoteEnvPlugin(allow_multiple_refs=allow_multiple_footnote_refs),
        ListEnvPlugin(),
        InlineFormatEnvPlugin(),
        UrlEnvPlugin(),
        SubfileEnvPlugin(),
    ]


@dataclass(frozen=True)
class FootnoteRef(Inline, NodePortal):
    portal_to: Backref


@dataclass(frozen=True)
class FootnoteContents(Block):
    anchor: Anchor
    contents: Inline


# Moons ago I considered replacing this with Backref. This should not be replaced with Backref,
# because it has specific context in how it is rendered. Renderer plugins may mutate the document
# and replace these with Backrefs if they so choose.
@dataclass(frozen=True)
class Citation(UserNode, Inline, InlineScopeBuilder):
    citenote: InlineScope | None
    citekeys: Set[str]
    anchor = None

    @override
    def child_nodes(self) -> InlineScope | None:
        return self.citenote

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        return Citation(citekeys=self.citekeys, citenote=inls)


@dataclass(frozen=True)
class CiteAuthor(Inline):
    citekey: str


class Bibliography(Block):
    pass


@dataclass(frozen=True)
class NamedUrl(UserNode, Inline, InlineScopeBuilder):
    name: Iterable[Inline] | None
    url: str
    anchor = None

    @override
    def child_nodes(self) -> Iterable[Inline] | None:
        return self.name

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        return NamedUrl(url=self.url, name=inls)


class DisplayListType(Enum):
    Enumerate = 0
    Itemize = 1


@dataclass(frozen=True)
class DisplayList(UserNode, Block):
    list_type: DisplayListType  # TODO could reuse Numbering from render.counters?
    contents: List[
        Union["DisplayList", "DisplayListItem"]
    ]  # TODO could replace DisplayListItem with Block? Auto-attach dots?
    anchor = None

    @override
    def child_nodes(self) -> Iterable[Block | Inline] | None:
        return self.contents


@dataclass(frozen=True)
class DisplayListItem(UserNode, Block):
    contents: BlockScope
    anchor = None

    @override
    def child_nodes(self) -> Iterable[Block | Inline] | None:
        return self.contents


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
class InlineFormatted(UserNode, Inline):
    format_type: InlineFormattingType
    contents: InlineScope
    anchor = None

    @override
    def child_nodes(self) -> Iterable[Block | Inline] | None:
        return self.contents


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


class CitationEnvPlugin(EnvPlugin):
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
        self, doc_env: DocEnv, fmt: FmtEnv, toplevel: Document
    ) -> Document:
        if not self._has_bib:
            toplevel.push_segment(
                DocSegment(
                    doc_env.heading1(num=False) @ "Bibliography",
                    BlockScope([Bibliography()]),
                    [],
                )
            )
        return toplevel

    @in_doc
    def cite(self, doc_env: DocEnv, *citekeys: str) -> Inline:
        citekey_set: set[str] = set(citekeys)
        for c in citekey_set:
            if not isinstance(c, str):
                raise ValueError(f"Inappropriate citation key: {c}. Must be a string")
        self._has_citations = True
        return Citation(citekeys=citekey_set, citenote=None)

    @pure_fmt
    def citeauthor(self, fmt: FmtEnv, citekey: str) -> Inline:
        return CiteAuthor(citekey)

    @in_doc
    def bibliography(self, doc_env: DocEnv) -> Bibliography:
        self._has_bib = True
        return Bibliography()


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


class InlineFormatEnvPlugin(EnvPlugin):
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


class ListEnvPlugin(EnvPlugin):
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


class UrlEnvPlugin(EnvPlugin):
    def _doc_nodes(self) -> Sequence[type[Block] | type[Inline]]:
        return (NamedUrl,)

    @pure_fmt
    def url(self, fmt: FmtEnv, url: str, name: Optional[str] = None) -> Inline:
        if not isinstance(url, str):
            raise ValueError(f"Url {url} must be a string")
        if name is not None and not isinstance(name, str):
            raise ValueError(f"Url name {name} must be a string if not None")
        return NamedUrl(
            name=(Text(name),) if name is not None else None,
            url=url,
        )


class SubfileEnvPlugin(EnvPlugin):
    @in_doc
    def subfile(self, doc_env: DocEnv, project_relative_path: str) -> TurnipTextSource:
        return doc_env.build_sys.resolve_turnip_text_source(project_relative_path)


class DocAnchors(EnvPlugin):
    """Responsible for keeping track of all the anchors in a document.

    Has enough information to convert a Backref to the Anchor that it refers to (inferring the kind)
    and retrieve information associated with the anchor.
    Allows document code to create anchors with `register_new_anchor()` or `register_new_anchor_with_float()`.
    Any backref can be converted to an Anchor (usually for rendering purposes) with `lookup_backref()`.
    The data associated with an Anchor in `register_new_anchor_with_float()` can be retrieved with an Anchor `lookup_anchor_float()` or a Backref to that Anchor `lookup_backref_float()`.

    Anchors can be created without knowing their ID, at which point this will generate an ID from a monotonic per-kind counter.
    To avoid overlap with user-defined IDs, user-defined IDs must contain at least one alphabetic latin character (upper or lowercase).

    This is a EnvPlugin so that it can use the @in_doc annotation to avoid creating new anchors after the document is frozen.
    """

    #

    _anchor_kind_counters: Dict[str, int]
    _anchor_id_to_possible_kinds: Dict[str, Dict[str, Anchor]]
    _anchored_floats: Dict[Anchor, Block]  # TODO rename floating_space

    # Anchor IDs, if they're user-defined, they must be
    _VALID_USER_ANCHOR_ID_REGEX = re.compile(r"\w*[a-zA-Z]\w*")

    def __init__(self) -> None:
        self._anchor_kind_counters = defaultdict(lambda: 1)
        self._anchor_id_to_possible_kinds = defaultdict(dict)
        self._anchored_floats = {}

    @in_doc
    def register_new_anchor(
        self, doc_env: DocEnv, kind: str, id: Optional[str]
    ) -> Anchor:
        """
        When inside the document, create a new anchor.
        """
        if id is None:
            id = str(self._anchor_kind_counters[kind])
        else:
            # Guarantee no overlap with auto-generated anchor IDs
            assert self._VALID_USER_ANCHOR_ID_REGEX.match(
                id
            ), "User-defined anchor IDs must have at least one alphabetic character"

        if self._anchor_id_to_possible_kinds[id].get(kind) is not None:
            raise ValueError(
                f"Tried to register anchor kind={kind}, id={id} when it already existed"
            )

        l = Anchor(
            kind=kind,
            id=id,
        )
        self._anchor_kind_counters[kind] += 1
        self._anchor_id_to_possible_kinds[id][kind] = l
        return l

    def register_new_anchor_with_float(
        self,
        kind: str,
        id: Optional[str],
        float_gen: Callable[[Anchor], Block],
    ) -> Anchor:
        a = self.register_new_anchor(kind, id)
        self._anchored_floats[a] = float_gen(a)
        return a

    def lookup_backref(self, backref: Backref) -> Anchor:
        """
        Should be called by renderers to resolve a backref into an anchor.
        The renderer can then retrieve the counters for the anchor.
        """

        if backref.id not in self._anchor_id_to_possible_kinds:
            raise ValueError(
                f"Backref {backref} refers to an ID '{backref.id}' with no anchor!"
            )

        possible_kinds = self._anchor_id_to_possible_kinds[backref.id]

        if backref.kind is None:
            if len(possible_kinds) != 1:
                raise ValueError(
                    f"Backref {backref} doesn't specify the kind of anchor it's referring to, and there are multiple with that ID: {possible_kinds}"
                )
            only_possible_anchor = next(iter(possible_kinds.values()))
            return only_possible_anchor
        else:
            if backref.kind not in possible_kinds:
                raise ValueError(
                    f"Backref {backref} specifies an anchor of kind {backref.kind}, which doesn't exist for ID {backref.id}: {possible_kinds}"
                )
            return possible_kinds[backref.kind]

    def lookup_anchor_float(self, anchor: Anchor) -> Optional[Block]:
        return self._anchored_floats.get(anchor)

    def lookup_backref_float(self, backref: Backref) -> Tuple[Anchor, Optional[Block]]:
        a = self.lookup_backref(backref)
        return a, self._anchored_floats.get(a)
