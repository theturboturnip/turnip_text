import uuid
from dataclasses import dataclass
from typing import (
    Any,
    Callable,
    Dict,
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
from turnip_text.renderers import CustomRenderFunc, Renderer, RendererPlugin
from turnip_text.renderers.dictify import dictify_pure_property
from turnip_text.renderers.markdown.base import RawMarkdown
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


class MarkdownCitationAsFootnotePlugin(RendererPlugin, CitationPluginInterface):
    _citations: Dict[str, Any]
    _referenced_citations: Set[str]

    def __init__(self) -> None:
        super().__init__()

        # TODO load citations from somewhere
        self._citations = {}
        self._referenced_citations = set()

    def _inline_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((Citation, self._render_citation),)

    def _postamble_handlers(self) -> Iterable[Tuple[str, Callable[[Renderer], str]]]:
        return ((self._BIBLIOGRAPHY_POSTAMBLE_ID, self._render_bibliography),)

    def _render_citation(self, renderer: Renderer, citation: Citation) -> str:
        # TODO what happens with unmarkdownable labels? e.g. labels with backslash or something. need to check that when loading.
        # TODO also maybe people wouldn't want those labels being exposed?
        return "".join(
            f"[^{label}]"
            if opt_note is None
            else f"\\([^{label}], {renderer.render_unescapedtext(opt_note)}\\)"
            for label, opt_note in citation.labels
        )

    def _render_bibliography(self, renderer: Renderer) -> str:
        # TODO actual reference rendering!
        return renderer.PARAGRAPH_SEP.join(
            f"[^{label}]: cite {label}" for label in self._referenced_citations
        )

    def cite(self, *labels: Union[str, Tuple[str, str]]) -> Inline:
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
    def citeauthor(self, label: str) -> Inline:
        return Citation([(label, None)])


class MarkdownCitationAsHTMLPlugin(RendererPlugin, CitationPluginInterface):
    _citations: Dict[str, Any]
    _referenced_citations: Set[str]

    def __init__(self) -> None:
        super().__init__()

        # TODO load citations from somewhere
        self._citations = {}
        self._referenced_citations = set()

    def _inline_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((Citation, self._render_citation),)

    def _postamble_handlers(self) -> Iterable[Tuple[str, Callable[[Renderer], str]]]:
        return ((self._BIBLIOGRAPHY_POSTAMBLE_ID, self._render_bibliography),)

    def _get_citation_shorthand(
        self, renderer: Renderer, label: str, note: Optional[UnescapedText]
    ) -> UnescapedText:
        # TODO could do e.g. numbering here
        if note:
            return UnescapedText(f"[{label}, {note.text}]")
        return UnescapedText(f"[{label}]")

    def _render_citation(self, renderer: Renderer, citation: Citation) -> str:
        # TODO what happens with unmarkdownable labels? e.g. labels with backslash or something. need to check that when loading.
        # TODO also maybe people wouldn't want those labels being exposed?
        return "".join(
            # f'<a href="#cite-{label}">{self._get_citation_shorthand(renderer, label, opt_note)}</a>'
            f"[{renderer.render_unescapedtext(self._get_citation_shorthand(renderer, label, opt_note))}](#cite-{label})"
            for label, opt_note in citation.labels
        )

    def _render_bibliography(self, renderer: Renderer) -> str:
        # TODO actual reference rendering!
        return renderer.PARAGRAPH_SEP.join(
            f'<a id="cite-{label}">{renderer.render_unescapedtext(self._get_citation_shorthand(renderer, label, None))}: cite {label}</a>'
            for label in self._referenced_citations
        )

    def cite(self, *labels: Union[str, Tuple[str, str]]) -> Inline:
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
    def citeauthor(self, label: str) -> Inline:
        return Citation([(label, None)])


class MarkdownFootnotePlugin(RendererPlugin, FootnotePluginInterface):
    _footnotes: Dict[str, Block]
    _footnote_ref_order: List[str]

    _MARKDOWN_FOOTNOTE_POSTAMBLE_ID = "MarkdownFootnotePlugin_Footnotes"

    def __init__(self) -> None:
        super().__init__()

        self._footnotes = {}
        self._footnote_ref_order = []

    def _inline_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((FootnoteAnchor, self._render_footnote_anchor),)

    def _postamble_handlers(self) -> Iterable[Tuple[str, Callable[[Renderer], str]]]:
        return ((self._MARKDOWN_FOOTNOTE_POSTAMBLE_ID, self._render_footnotes),)

    def _render_footnote_anchor(
        self, renderer: Renderer, footnote: FootnoteAnchor
    ) -> str:
        footnote_num = self._footnote_ref_order.index(footnote.label)
        return f"[^{footnote_num + 1}]"

    def _render_footnotes(self, renderer: Renderer) -> str:
        return renderer.PARAGRAPH_SEP.join(
            # TODO render with indent
            f"[^{num + 1}]: " + renderer.render_block(self._footnotes[label])
            for num, label in enumerate(self._footnote_ref_order)
        )

    def _add_footnote_reference(self, label: str):
        if label not in self._footnote_ref_order:
            self._footnote_ref_order.append(label)

    @dictify_pure_property
    def footnote(self) -> InlineScopeBuilder:
        @inline_scope_builder
        def footnote_builder(contents: InlineScope) -> Inline:
            label = str(uuid.uuid4())
            self._add_footnote_reference(label)
            self._footnotes[label] = Paragraph([Sentence([contents])])
            return FootnoteAnchor(label)

        return footnote_builder

    def footnote_ref(self, label: str) -> Inline:
        self._add_footnote_reference(label)
        return FootnoteAnchor(label)

    def footnote_text(self, label: str) -> BlockScopeBuilder:
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


class MarkdownSectionPlugin(RendererPlugin, SectionPluginInterface):
    def _block_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((HeadedBlock, self._render_headed_block),)

    def _render_headed_block(self, renderer: Renderer, block: HeadedBlock) -> str:
        if block.label:
            raise NotImplementedError("Section labelling not supported")

        header = "#" * block.level + " "
        if block.use_num:
            header += ".".join(str(n) for n in block.num)
            header += " "
        header += renderer.render_unescapedtext(block.name)

        return (
            header + renderer.PARAGRAPH_SEP + renderer.render_blockscope(block.contents)
        )

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

    def section(
        self, name: str, label: Optional[str] = None, num: bool = True
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

    def subsection(
        self, name: str, label: Optional[str] = None, num: bool = True
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

    def subsubsection(
        self, name: str, label: Optional[str] = None, num: bool = True
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


class MarkdownFormatPlugin(RendererPlugin, FormatPluginInterface):
    def _inline_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((Formatted, self._render_formatted),)

    def _render_formatted(self, renderer: Renderer, item: Formatted) -> str:
        return (
            item.format_type
            + renderer.render_inlinescope(item.items)
            + item.format_type
        )

    @dictify_pure_property
    def emph(self) -> InlineScopeBuilder:
        @inline_scope_builder
        def emph_builder(items: InlineScope) -> Inline:
            # Use single underscore for italics, double asterisks for bold, double underscore for underlining?
            return Formatted("_", items)

        return emph_builder

    DQUOTE = RawMarkdown('"')

    @dictify_pure_property
    def enquote(self) -> InlineScopeBuilder:
        @inline_scope_builder
        def enquote_builder(items: InlineScope) -> Inline:
            return InlineScope(
                [MarkdownFormatPlugin.DQUOTE]
                + list(items)
                + [MarkdownFormatPlugin.DQUOTE]
            )

        return enquote_builder


class MarkdownListPlugin(RendererPlugin):
    def _block_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((DisplayList, self._render_list),)

    def _render_list(self, renderer: Renderer, list: DisplayList) -> str:
        # TODO indents!
        # If list items are multiline, right now all lines after the first will NOT be indented and thus not counted as part of the list item.
        if list.numbered:
            return renderer.SENTENCE_SEP.join(
                f"{idx+1}. " + renderer.render_block(item.contents)
                for idx, item in enumerate(list.items)
            )
        else:
            return renderer.SENTENCE_SEP.join(
                f"- " + renderer.render_block(item.contents)
                for idx, item in enumerate(list.items)
            )

    @dictify_pure_property
    def enumerate(self) -> BlockScopeBuilder:
        @block_scope_builder
        def enumerate_builder(contents: BlockScope) -> Block:
            items = list(contents)
            if any(not isinstance(x, DisplayListItem) for x in items):
                raise TypeError(
                    f"Found blocks in this list that were not list [item]s!"
                )
            return DisplayList(numbered=True, items=cast(List[DisplayListItem], items))

        return enumerate_builder

    @dictify_pure_property
    def itemize(self) -> BlockScopeBuilder:
        @block_scope_builder
        def itemize_builder(contents: BlockScope) -> Block:
            items = list(contents)
            if any(not isinstance(x, DisplayListItem) for x in items):
                raise TypeError(
                    f"Found blocks in this list that were not list [item]s!"
                )
            return DisplayList(numbered=False, items=cast(List[DisplayListItem], items))

        return itemize_builder

    @dictify_pure_property
    def item(self) -> BlockScopeBuilder:
        @block_scope_builder
        def item_builder(block_scope: BlockScope) -> Block:
            return DisplayListItem(block_scope)

        return item_builder


class MarkdownUrlPlugin(RendererPlugin):
    def _inline_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((NamedUrl, self._render_url),)

    def _render_url(self, renderer: Renderer, url: NamedUrl) -> str:
        if url.name is None:
            # Set the "name" of the URL to the text of the URL - escaped so it can be read as normal markdown
            escaped_url_name = renderer.render_unescapedtext(UnescapedText(url.url))
        else:
            escaped_url_name = renderer.render_inlinescope(url.name)
        return f"[{escaped_url_name}]({url.url})"

    def url(self, url: str, name: Optional[str] = None) -> Inline:
        return NamedUrl(
            url, name=InlineScope([UnescapedText(name)]) if name is not None else None
        )
