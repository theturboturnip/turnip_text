import uuid
from dataclasses import dataclass
from typing import Any, Callable, Dict, Iterable, List, Optional, Tuple, Union, cast

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
from turnip_text.renderers import Plugin, Renderer, stateful, stateless
from turnip_text.renderers.stateful import (
    CustomEmitDispatch,
    MutableState,
    StatelessContext,
)
from turnip_text.renderers.std_plugins import (
    CitationPluginInterface,
    CiteKey,
    FootnotePluginInterface,
    FormatPluginInterface,
    SectionPluginInterface,
)

from .base import LatexRenderer, RawLatex

CiteKeyWithOptionNote = Tuple[CiteKey, Optional[UnescapedText]]


@dataclass(frozen=True)
class FootnoteAnchor(Inline):
    label: str


@dataclass(frozen=True)
class HeadedBlock(Block):
    latex_name: str
    name: UnescapedText
    contents: BlockScope
    label: Optional[str] = None
    num: bool = True


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
    mode: str


@dataclass(frozen=True)
class DisplayListItem(Block):
    item: Block


@dataclass(frozen=True)
class Formatted(Inline):
    format_type: str  # e.g. "emph"
    items: InlineScope


class LatexCitationPlugin(Plugin[LatexRenderer], CitationPluginInterface):
    # TODO require biblatex

    _citations: Dict[str, Any]

    def __init__(self) -> None:
        super().__init__()

        # TODO load citations from somewhere
        self._citations = {}

    def _add_emitters(self, handler: CustomEmitDispatch[LatexRenderer]) -> None:
        handler.add_custom_inline(Citation, self._emit_citation)

    def _postamble_handlers(self) -> Iterable[Tuple[str, Callable[[LatexRenderer], None]]]:
        return ((self._BIBLIOGRAPHY_POSTAMBLE_ID, self._emit_bibliography),)

    def _emit_citation(
        self,
        renderer: LatexRenderer,
        ctx: StatelessContext[LatexRenderer],
        citation: Citation,
    ) -> None:
        if any(note for _, note in citation.labels):
            # We can't add a citenote for multiple citations at a time in Latex - split individually
            for key, note in citation.labels:
                renderer.emit_macro("cite")
                if note:
                    renderer.emit_sqr_bracketed(note)
                renderer.emit_braced(str(key))
        else:
            renderer.emit_macro("cite")
            renderer.emit_braced(",".join(key for key, _ in citation.labels))

    def _emit_bibliography(self, renderer: LatexRenderer) -> None:
        renderer.emit("""{
\\raggedright
\\printbibliography
}""")

    @stateless
    def cite(
        self, ctx: StatelessContext[LatexRenderer], *labels: Union[str, Tuple[str, str]]
    ) -> Inline:
        # Convert ["label"] to [("label", None)] so Citation has a consistent format
        adapted_labels = [
            (label, None)
            if isinstance(label, str)
            else (label[0], UnescapedText(label[1]))
            for label in labels
        ]
        return Citation(adapted_labels)

    # TODO make this output \citeauthor
    @stateless
    def citeauthor(self, ctx: StatelessContext[LatexRenderer], label: str) -> Inline:
        return Citation([(label, None)])


class LatexFootnotePlugin(Plugin[LatexRenderer], FootnotePluginInterface):
    _footnotes: Dict[str, Block]

    def __init__(self) -> None:
        super().__init__()

        self._footnotes = {}

    def _add_emitters(self, handler: CustomEmitDispatch[LatexRenderer]) -> None:
        handler.add_custom_inline(FootnoteAnchor, self._emit_footnote_anchor)

    def _emit_footnote_anchor(
        self,
        renderer: LatexRenderer,
        ctx: StatelessContext[LatexRenderer],
        footnote: FootnoteAnchor,
    ) -> None:
        # TODO - intelligent footnotetext placement using floats?
        renderer.emit_macro("footnote")
        renderer.emit_braced(self._footnotes[footnote.label])

    @property
    @stateful
    def footnote(self, state: MutableState[LatexRenderer]) -> InlineScopeBuilder:
        @inline_scope_builder
        def footnote_builder(contents: InlineScope) -> Inline:
            label = str(uuid.uuid4())
            self._footnotes[label] = Paragraph([Sentence([contents])])
            return FootnoteAnchor(label)

        return footnote_builder

    @stateless
    def footnote_ref(self, ctx: StatelessContext[LatexRenderer], label: str) -> Inline:
        return FootnoteAnchor(label)

    @stateful
    def footnote_text(
        self, state: MutableState[LatexRenderer], label: str
    ) -> BlockScopeBuilder:
        # Store the contents of a block scope and associate them with a specific footnote label
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Optional[Block]:
            self._footnotes[label] = contents
            return None

        return handle_block_contents


class LatexSectionPlugin(Plugin[LatexRenderer], SectionPluginInterface):
    _pagebreak_before: List[str]

    def __init__(self, pagebreak_before: List[str] = []) -> None:
        super().__init__()
        self._pagebreak_before = pagebreak_before

    def _add_emitters(self, handler: CustomEmitDispatch[LatexRenderer]) -> None:
        handler.add_custom_block(HeadedBlock, self._emit_headed_block)

    def _emit_headed_block(
        self,
        renderer: LatexRenderer,
        ctx: StatelessContext[LatexRenderer],
        block: HeadedBlock,
    ) -> None:
        if block.latex_name in self._pagebreak_before:
            renderer.emit_raw("\\pagebreak\n")
        if block.num:
            renderer.emit_macro(block.latex_name) # i.e. r"\section"
        else:
            renderer.emit_macro(block.latex_name + "*")
        renderer.emit_braced(block.name) # i.e. r"\section*" + "{Section Name}"
        if block.label:
            renderer.emit_macro("label")
            renderer.emit_braced(block.label) # i.e. r"\section*{Section Name}" + r"\label{block_label}"
        renderer.emit_break_paragraph()
        renderer.emit_blockscope(block.contents)

    @stateful
    def section(
        self,
        state: MutableState[LatexRenderer],
        name: str,
        label: Optional[str] = None,
        num: bool = True,
    ) -> BlockScopeBuilder:
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            return HeadedBlock(
                latex_name="section",
                name=UnescapedText(name),
                label=label,
                num=num,
                contents=contents,
            )

        return handle_block_contents

    @stateful
    def subsection(
        self,
        state: MutableState[LatexRenderer],
        name: str,
        label: Optional[str] = None,
        num: bool = True,
    ) -> BlockScopeBuilder:
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            return HeadedBlock(
                latex_name="subsection",
                name=UnescapedText(name),
                label=label,
                num=num,
                contents=contents,
            )

        return handle_block_contents

    @stateful
    def subsubsection(
        self,
        state: MutableState[LatexRenderer],
        name: str,
        label: Optional[str] = None,
        num: bool = True,
    ) -> BlockScopeBuilder:
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            return HeadedBlock(
                latex_name="subsubsection",
                name=UnescapedText(name),
                label=label,
                num=num,
                contents=contents,
            )

        return handle_block_contents


@inline_scope_builder
def emph_builder(items: InlineScope) -> Inline:
    return Formatted("emph", items)


@inline_scope_builder
def italic_builder(items: InlineScope) -> Inline:
    return Formatted("textit", items)


@inline_scope_builder
def bold_builder(items: InlineScope) -> Inline:
    return Formatted("textbf", items)


class LatexFormatPlugin(Plugin[LatexRenderer], FormatPluginInterface):
    def _add_emitters(self, handler: CustomEmitDispatch[LatexRenderer]) -> None:
        handler.add_custom_inline(Formatted, self._emit_formatted)

    def _emit_formatted(
        self,
        renderer: LatexRenderer,
        ctx: StatelessContext[LatexRenderer],
        item: Formatted,
    ) -> None:
        renderer.emit_macro(item.format_type)
        renderer.emit_braced(item.items)

    emph = emph_builder
    italic = italic_builder
    bold = bold_builder

    OPEN_DQUOTE = RawLatex("``")
    CLOS_DQUOTE = RawLatex("''")

    @property
    @stateless
    def enquote(self, ctx: StatelessContext[LatexRenderer]) -> InlineScopeBuilder:
        @inline_scope_builder
        def enquote_builder(items: InlineScope) -> Inline:
            return InlineScope(
                [LatexFormatPlugin.OPEN_DQUOTE]
                + list(items)
                + [LatexFormatPlugin.CLOS_DQUOTE]
            )

        return enquote_builder


class LatexListPlugin(Plugin[LatexRenderer]):
    indent_list_items: bool = True

    def __init__(self, indent_list_items: bool = True):
        self.indent_list_items = indent_list_items

    def _add_emitters(self, handler: CustomEmitDispatch[LatexRenderer]) -> None:
        handler.add_custom_block(DisplayList, self._emit_list)
        handler.add_custom_block(DisplayListItem, self._emit_list_item)

    def _emit_list(
        self,
        renderer: LatexRenderer,
        ctx: StatelessContext[LatexRenderer],
        list: DisplayList,
    ) -> None:
        renderer.emit_env_begin(list.mode)
        renderer.emit(*list.items, joiner=renderer.emit_break_paragraph)
        renderer.emit_env_end(list.mode)

    def _emit_list_item(
        self,
        renderer: LatexRenderer,
        ctx: StatelessContext[LatexRenderer],
        list_item: DisplayListItem,
    ) -> None:
        renderer.emit_macro("item")
        # Put {} after \item so square brackets at the start of render_block don't get swallowed as arguments
        renderer.emit("{} ")
        indent_width = len("\\item{} ")
        renderer.push_indent(indent_width)
        renderer.emit(list_item.item)
        renderer.pop_indent(indent_width)

    @property
    @stateless
    def enumerate(self, ctx: StatelessContext[LatexRenderer]) -> BlockScopeBuilder:
        @block_scope_builder
        def enumerate_builder(contents: BlockScope) -> Block:
            items = list(contents)
            if any(not isinstance(x, DisplayListItem) for x in items):
                raise TypeError(
                    f"Found blocks in this list that were not list [item]s!"
                )
            return DisplayList(
                mode="enumerate", items=cast(List[DisplayListItem], items)
            )

        return enumerate_builder

    @property
    @stateless
    def itemize(self, ctx: StatelessContext[LatexRenderer]) -> BlockScopeBuilder:
        @block_scope_builder
        def itemize_builder(contents: BlockScope) -> Block:
            items = list(contents)
            if any(not isinstance(x, DisplayListItem) for x in items):
                raise TypeError(
                    f"Found blocks in this list that were not list [item]s!"
                )
            return DisplayList(mode="itemize", items=cast(List[DisplayListItem], items))

        return itemize_builder

    @property
    @stateless
    def item(self, ctx: StatelessContext[LatexRenderer]) -> BlockScopeBuilder:
        @block_scope_builder
        def item_builder(block_scope: BlockScope) -> Block:
            return DisplayListItem(block_scope)

        return item_builder


class LatexUrlPlugin(Plugin[LatexRenderer]):
    # TODO add dependency on hyperref!!

    def _add_emitters(self, handler: CustomEmitDispatch[LatexRenderer]) -> None:
        handler.add_custom_inline(NamedUrl, self._emit_url)

    def _emit_url(
        self, renderer: LatexRenderer, ctx: StatelessContext[LatexRenderer], url: NamedUrl
    ) -> None:
        assert "}" not in url.url
        assert "#" not in url.url
        if url.name is None:
            renderer.emit_macro("url")
            renderer.emit_braced(url.url)
        else:
            renderer.emit_macro("href")
            renderer.emit_braced(url.url)
            renderer.emit_braced(url.name)

    @stateless
    def url(
        self, ctx: StatelessContext[LatexRenderer], url: str, name: Optional[str] = None
    ) -> Inline:
        return NamedUrl(
            url, name=InlineScope([UnescapedText(name)]) if name is not None else None
        )
