import uuid
from dataclasses import dataclass
from typing import (
    Any,
    Callable,
    Dict,
    Generator,
    Iterable,
    List,
    Optional,
    Set,
    Tuple,
    Union,
    cast,
)

from turnip_text import (
    Block,
    BlockScope,
    BlockScopeBuilder,
    Inline,
    InlineScope,
    InlineScopeBuilder,
    Paragraph,
    Sentence,
    UnescapedText,
)
from turnip_text.helpers import block_scope_builder, inline_scope_builder
from turnip_text.renderers import (
    CustomEmitDispatch,
    MutableState,
    Plugin,
    Renderer,
    StatelessContext,
    stateful,
    stateless,
)
from turnip_text.renderers.markdown.base import MarkdownRenderer, RawMarkdown
from turnip_text.renderers.std_plugins import (
    CitationPluginInterface,
    CiteKey,
    FootnotePluginInterface,
    FormatPluginInterface,
    SectionPluginInterface,
)

CiteKeyWithOptionNote = Tuple[CiteKey, Optional[UnescapedText]]


@dataclass(frozen=True)
class FootnoteAnchor(Inline):
    label: str


@dataclass(frozen=True)
class HeadedBlock(Block):
    level: int
    name: UnescapedText
    contents: BlockScope
    # Numbering
    use_num: bool  # Whether to number the heading or not
    num: Tuple[int, ...]  # The actual number of the heading
    # Labelling
    label: Optional[str] = None


@dataclass(frozen=True)
class Citation(Inline):
    # List of (label, note?)
    labels: List[CiteKeyWithOptionNote]


@dataclass(frozen=True)
class NamedUrl(Inline, InlineScopeBuilder):
    url: str
    name: Optional[InlineScope] = None

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        assert self.name is None
        return NamedUrl(self.url, inls)


@dataclass(frozen=True)
class DisplayList(Block):
    # TODO allow nested lists
    # items: List[Union[BlockNode, List]]
    items: List["DisplayListItem"]
    numbered: bool


@dataclass(frozen=True)
class DisplayListItem(Block):
    contents: Block


@dataclass(frozen=True)
class Formatted(Inline):
    format_type: str  # e.g. "*", "**"
    items: InlineScope


class MarkdownCitationAsFootnotePlugin(
    Plugin[MarkdownRenderer], CitationPluginInterface
):
    _citations: Dict[str, Any]
    _referenced_citations: Set[str]

    def __init__(self) -> None:
        super().__init__()

        # TODO load citations from somewhere
        self._citations = {}
        self._referenced_citations = set()

    def _add_emitters(self, handler: CustomEmitDispatch[MarkdownRenderer]) -> None:
        handler.add_custom_inline(Citation, self._emit_citation)

    def _postamble_handlers(
        self,
    ) -> Iterable[Tuple[str, Callable[[MarkdownRenderer], None]]]:
        return ((self._BIBLIOGRAPHY_POSTAMBLE_ID, self._emit_bibliography),)

    def _emit_citation(
        self,
        renderer: MarkdownRenderer,
        ctx: StatelessContext[MarkdownRenderer],
        citation: Citation,
    ) -> None:
        # TODO what happens with unmarkdownable labels? e.g. labels with backslash or something. need to check that when loading.
        # TODO also maybe people wouldn't want those labels being exposed?

        for label, opt_note in citation.labels:
            if opt_note is None:
                renderer.emit_raw(f"[^{label}]")
            else:
                renderer.emit(
                    f"\\([^{label}], ",
                    opt_note,
                    f"\\)"
                )

    def _emit_bibliography(self, renderer: MarkdownRenderer) -> None:
        # TODO actual reference rendering!
        def bib_gen() -> Generator[None, None, None]:
            for label in self._referenced_citations:
                renderer.emit(f"[^{label}]: ", UnescapedText(f"cite {label}"))
                yield

        renderer.emit_join_gen(bib_gen(), renderer.emit_break_paragraph)

    @stateful
    def cite(
        self,
        state: MutableState[MarkdownRenderer],
        *labels: Union[str, Tuple[str, str]],
    ) -> Inline:
        # Convert ["label"] to [("label", None)] so Citation has a consistent format
        adapted_labels = [
            (label, None)
            if isinstance(label, str)
            else (label[0], UnescapedText(label[1]))
            for label in labels
        ]

        self._referenced_citations.update(label for label, _ in adapted_labels)

        return Citation(adapted_labels)

    @stateless
    # TODO make this output \citeauthor
    def citeauthor(self, ctx: StatelessContext[MarkdownRenderer], label: str) -> Inline:
        return Citation([(label, None)])


class MarkdownCitationAsHTMLPlugin(Plugin[MarkdownRenderer], CitationPluginInterface):
    _citations: Dict[str, Any]
    _referenced_citations: Set[str]

    def __init__(self) -> None:
        super().__init__()

        # TODO load citations from somewhere
        self._citations = {}
        self._referenced_citations = set()

    def _add_emitters(self, handler: CustomEmitDispatch[MarkdownRenderer]) -> None:
        handler.add_custom_inline(Citation, self._emit_citation)

    def _postamble_handlers(
        self,
    ) -> Iterable[Tuple[str, Callable[[MarkdownRenderer], None]]]:
        return ((self._BIBLIOGRAPHY_POSTAMBLE_ID, self._emit_bibliography),)

    def _get_citation_shorthand(
        self, label: str, note: Optional[UnescapedText]
    ) -> UnescapedText:
        # TODO could do e.g. numbering here
        if note:
            return UnescapedText(f"[{label}, {note.text}]")
        return UnescapedText(f"[{label}]")

    def _emit_citation(
        self,
        renderer: MarkdownRenderer,
        ctx: StatelessContext[MarkdownRenderer],
        citation: Citation,
    ) -> None:
        # TODO what happens with unmarkdownable labels? e.g. labels with backslash or something. need to check that when loading.
        # TODO also maybe people wouldn't want those labels being exposed?

        for label, opt_note in citation.labels:
            renderer.emit(ctx.url(f"#cite-{label}") @ self._get_citation_shorthand(label, opt_note))

    def _emit_bibliography(self, renderer: MarkdownRenderer) -> None:
        # TODO actual reference rendering!
        def emit_individual(label: str) -> None:
            renderer.emit(
                f'<a id="cite-{label}">',
                self._get_citation_shorthand(label, None),
                f" : cite {label}</a>"
            )

        renderer.emit_join(
            emit_individual,
            self._referenced_citations,
            renderer.emit_break_paragraph,
        )


    @stateful
    def cite(
        self,
        state: MutableState[MarkdownRenderer],
        *labels: Union[str, Tuple[str, str]],
    ) -> Inline:
        # Convert ["label"] to [("label", None)] so Citation has a consistent format
        adapted_labels = [
            (label, None)
            if isinstance(label, str)
            else (label[0], UnescapedText(label[1]))
            for label in labels
        ]

        self._referenced_citations.update(label for label, _ in adapted_labels)

        return Citation(adapted_labels)

    # TODO make this output \citeauthor
    @stateful
    def citeauthor(self, state: MutableState[MarkdownRenderer], label: str) -> Inline:
        return Citation([(label, None)])


class MarkdownFootnotePlugin(Plugin[MarkdownRenderer], FootnotePluginInterface):
    _footnotes: Dict[str, Block]
    _footnote_ref_order: List[str]

    _MARKDOWN_FOOTNOTE_POSTAMBLE_ID = "MarkdownFootnotePlugin_Footnotes"

    def __init__(self) -> None:
        super().__init__()

        self._footnotes = {}
        self._footnote_ref_order = []

    def _add_emitters(self, handler: CustomEmitDispatch[MarkdownRenderer]) -> None:
        handler.add_custom_inline(FootnoteAnchor, self._emit_footnote_anchor)

    def _postamble_handlers(
        self,
    ) -> Iterable[Tuple[str, Callable[[MarkdownRenderer], None]]]:
        return ((self._MARKDOWN_FOOTNOTE_POSTAMBLE_ID, self._emit_footnotes),)

    def _emit_footnote_anchor(
        self,
        renderer: MarkdownRenderer,
        ctx: StatelessContext[MarkdownRenderer],
        footnote: FootnoteAnchor,
    ) -> None:
        footnote_num = self._footnote_ref_order.index(footnote.label)
        renderer.emit_raw(f"[^{footnote_num + 1}]")

    def _emit_footnotes(self, renderer: MarkdownRenderer) -> None:
        def emit_individual(num_label: Tuple[int, str]) -> None:
            num, label = num_label
            renderer.emit(f"[^{num + 1}]: ", self._footnotes[label])

        return renderer.emit_join(
            emit_individual,
            enumerate(self._footnote_ref_order),
            renderer.emit_break_paragraph,
        )

    def _add_footnote_reference(self, label: str) -> None:
        if label not in self._footnote_ref_order:
            self._footnote_ref_order.append(label)

    @property
    @stateful
    def footnote(self, state: MutableState[MarkdownRenderer]) -> InlineScopeBuilder:
        @inline_scope_builder
        def footnote_builder(contents: InlineScope) -> Inline:
            label = str(uuid.uuid4())
            self._add_footnote_reference(label)
            self._footnotes[label] = Paragraph([Sentence([contents])])
            return FootnoteAnchor(label)

        return footnote_builder

    @stateful
    def footnote_ref(self, state: MutableState[MarkdownRenderer], label: str) -> Inline:
        self._add_footnote_reference(label)
        return FootnoteAnchor(label)

    @stateful
    def footnote_text(
        self, state: MutableState[MarkdownRenderer], label: str
    ) -> BlockScopeBuilder:
        # Return a callable which is invoked with the contents of the following inline scope
        # Example usage:
        # [footnote_text("label")]{text}
        # equivalent to
        # [footnote_text("label")(r"text")]
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Optional[Block]:
            self._footnotes[label] = contents
            return None

        return handle_block_contents


class MarkdownSectionPlugin(Plugin[MarkdownRenderer], SectionPluginInterface):
    def _add_emitters(self, handler: CustomEmitDispatch[MarkdownRenderer]) -> None:
        handler.add_custom_block(HeadedBlock, self._emit_headed_block)

    def _emit_headed_block(
        self,
        renderer: MarkdownRenderer,
        ctx: StatelessContext[MarkdownRenderer],
        block: HeadedBlock,
    ) -> None:
        if block.label:
            raise NotImplementedError("Section labelling not supported")

        header = "#" * block.level + " "
        if block.use_num:
            header += ".".join(str(n) for n in block.num)
            header += " "
        renderer.emit(
            header,
            block.name
        )
        renderer.emit_break_paragraph()
        renderer.emit_blockscope(block.contents)

    # [section_num, subsection_num... etc]
    _current_number: List[int]

    def __init__(self) -> None:
        super().__init__()
        self._current_number = []

    def _update_number(self, current_level: int) -> Tuple[int, ...]:
        """Update the structure number based on a newly-encountered header at level current_level.

        i.e. if encountering a new section, call _update_number(2), which will a) increment the subsection number and b) return a tuple of (section_num, subsection_num) which can be used in rendering later.
        """

        # Step 1: pad self._current_number out to current_level elements with 0s
        self._current_number += (current_level - len(self._current_number)) * [0]
        # Step 2: increment self._current_number[level - 1] and reset all numbers beyond that to 0
        # e.g. for a plain section, level = 1 => increment current_number[0] and zero out current_number[1:]
        self._current_number[current_level - 1] += 1
        for i in range(current_level, len(self._current_number)):
            self._current_number[i] = 0
        # Step 3: return a tuple of all elements up to current_level
        return tuple(self._current_number[:current_level])

    @stateful
    def section(
        self,
        state: MutableState[MarkdownRenderer],
        name: str,
        label: Optional[str] = None,
        num: bool = True,
    ) -> BlockScopeBuilder:
        # Increment the section number here, not at the end of the block
        section_num = self._update_number(1)

        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            return HeadedBlock(
                level=1,
                name=UnescapedText(name),
                label=label,
                use_num=num,
                num=section_num,
                contents=contents,
            )

        return handle_block_contents

    @stateful
    def subsection(
        self,
        state: MutableState[MarkdownRenderer],
        name: str,
        label: Optional[str] = None,
        num: bool = True,
    ) -> BlockScopeBuilder:
        # Increment the section number here, not at the end of the block
        section_num = self._update_number(2)

        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            return HeadedBlock(
                level=2,
                name=UnescapedText(name),
                label=label,
                use_num=num,
                num=section_num,
                contents=contents,
            )

        return handle_block_contents

    @stateful
    def subsubsection(
        self,
        state: MutableState[MarkdownRenderer],
        name: str,
        label: Optional[str] = None,
        num: bool = True,
    ) -> BlockScopeBuilder:
        # Increment the section number here, not at the end of the block
        section_num = self._update_number(3)

        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            return HeadedBlock(
                level=3,
                name=UnescapedText(name),
                label=label,
                use_num=num,
                num=section_num,
                contents=contents,
            )

        return handle_block_contents


@inline_scope_builder
def emph_builder(items: InlineScope) -> Inline:
    return italic_builder.build_from_inlines(items)


@inline_scope_builder
def italic_builder(items: InlineScope) -> Inline:
    # Use single underscore for italics, double asterisks for bold, double underscore for underlining?
    return Formatted("_", items)


@inline_scope_builder
def bold_builder(items: InlineScope) -> Inline:
    return Formatted("**", items)


class MarkdownFormatPlugin(Plugin[MarkdownRenderer], FormatPluginInterface):
    def _add_emitters(self, handler: CustomEmitDispatch[MarkdownRenderer]) -> None:
        handler.add_custom_inline(Formatted, self._emit_formatted)

    def _emit_formatted(
        self,
        renderer: MarkdownRenderer,
        ctx: StatelessContext[MarkdownRenderer],
        item: Formatted,
    ) -> None:
        renderer.emit(
            item.format_type,
            item.items,
            item.format_type,
        )

    emph = emph_builder
    italic = italic_builder
    bold = bold_builder

    DQUOTE = RawMarkdown('"')

    @property
    @stateless
    def enquote(self, ctx: StatelessContext[MarkdownRenderer]) -> InlineScopeBuilder:
        @inline_scope_builder
        def enquote_builder(items: InlineScope) -> Inline:
            return InlineScope(
                [MarkdownFormatPlugin.DQUOTE]
                + list(items)
                + [MarkdownFormatPlugin.DQUOTE]
            )

        return enquote_builder


class MarkdownListPlugin(Plugin[MarkdownRenderer]):
    def _add_emitters(self, handler: CustomEmitDispatch[MarkdownRenderer]) -> None:
        handler.add_custom_block(DisplayList, self._emit_list)

    def _emit_list(
        self,
        renderer: MarkdownRenderer,
        ctx: StatelessContext[MarkdownRenderer],
        list: DisplayList,
    ) -> None:
        if list.numbered:
            def emit_numbered() -> Generator[None, None, None]:
                for idx, item in enumerate(list.items):
                    indent = f"{idx+1}. "
                    renderer.emit_raw(indent)
                    with renderer.indent(len(indent)):
                        renderer.emit_block(item.contents)
                    yield None

            renderer.emit_join_gen(emit_numbered(), renderer.emit_break_sentence)
        else:
            def emit_dashed() -> Generator[None, None, None]:
                for idx, item in enumerate(list.items):
                    indent = f"- "
                    renderer.emit_raw(indent)
                    with renderer.indent(len(indent)):
                        renderer.emit_block(item.contents)
                    yield None

            renderer.emit_join_gen(emit_dashed(), renderer.emit_break_sentence)

    @property
    @stateless
    def enumerate(self, ctx: StatelessContext[MarkdownRenderer]) -> BlockScopeBuilder:
        @block_scope_builder
        def enumerate_builder(contents: BlockScope) -> Block:
            items = list(contents)
            if any(not isinstance(x, DisplayListItem) for x in items):
                raise TypeError(
                    f"Found blocks in this list that were not list [item]s!"
                )
            return DisplayList(numbered=True, items=cast(List[DisplayListItem], items))

        return enumerate_builder

    @property
    @stateless
    def itemize(self, ctx: StatelessContext[MarkdownRenderer]) -> BlockScopeBuilder:
        @block_scope_builder
        def itemize_builder(contents: BlockScope) -> Block:
            items = list(contents)
            if any(not isinstance(x, DisplayListItem) for x in items):
                raise TypeError(
                    f"Found blocks in this list that were not list [item]s!"
                )
            return DisplayList(numbered=False, items=cast(List[DisplayListItem], items))

        return itemize_builder

    @property
    @stateless
    def item(self, ctx: StatelessContext[MarkdownRenderer]) -> BlockScopeBuilder:
        @block_scope_builder
        def item_builder(block_scope: BlockScope) -> Block:
            return DisplayListItem(block_scope)

        return item_builder


class MarkdownUrlPlugin(Plugin[MarkdownRenderer]):
    def _add_emitters(self, handler: CustomEmitDispatch[MarkdownRenderer]) -> None:
        handler.add_custom_inline(NamedUrl, self._emit_url)

    def _emit_url(
        self,
        renderer: MarkdownRenderer,
        ctx: StatelessContext[MarkdownRenderer],
        url: NamedUrl,
    ) -> None:
        assert ")" not in url.url
        renderer.emit_raw("[")
        if url.name is None:
            # Set the "name" of the URL to the text of the URL - escaped so it can be read as normal markdown
            renderer.emit_unescapedtext(UnescapedText(url.url))
        else:
            renderer.emit_inlinescope(url.name)
        renderer.emit_raw(f"]({url.url})")

    @stateless
    def url(
        self,
        ctx: StatelessContext[MarkdownRenderer],
        url: str,
        name: Optional[str] = None,
    ) -> Inline:
        return NamedUrl(
            url, name=InlineScope([UnescapedText(name)]) if name is not None else None
        )
