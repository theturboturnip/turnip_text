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
    CustomRenderDispatch,
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

    def _add_renderers(self, handler: CustomRenderDispatch[LatexRenderer]):
        handler.add_custom_inline(Citation, self._render_citation)

    def _postamble_handlers(self) -> Iterable[Tuple[str, Callable[[Renderer], str]]]:
        return ((self._BIBLIOGRAPHY_POSTAMBLE_ID, self._render_bibliography),)

    def _render_citation(
        self,
        renderer: LatexRenderer,
        ctx: StatelessContext[LatexRenderer],
        citation: Citation,
    ) -> str:
        if any(note for _, note in citation.labels):
            # We can't add a citenote for multiple citations at a time in Latex - split individually
            data = ""
            for key, note in citation.labels:
                if note:
                    rendered_note = renderer.render_unescapedtext(note)
                    data += f"\\cite[{rendered_note}]{{{key}}}"
                else:
                    data += f"\\cite{{{key}}}"
            return data
        else:
            data = "\\cite{" + ",".join(key for key, _ in citation.labels) + "}"
            return data

    def _render_bibliography(self, renderer: Renderer) -> str:
        return """{
\\raggedright
\\printbibliography
}"""

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

    def _add_renderers(self, handler: CustomRenderDispatch[LatexRenderer]):
        handler.add_custom_inline(FootnoteAnchor, self._render_footnote_anchor)

    def _render_footnote_anchor(
        self,
        renderer: LatexRenderer,
        ctx: StatelessContext[LatexRenderer],
        footnote: FootnoteAnchor,
    ) -> str:
        # TODO - intelligent footnotetext placement using floats?
        rendered_footnotetext = renderer.render_block(self._footnotes[footnote.label])
        return f"\\footnote{{{rendered_footnotetext}}}"

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

    def _add_renderers(self, handler: CustomRenderDispatch[LatexRenderer]):
        handler.add_custom_block(HeadedBlock, self._render_headed_block)

    def _render_headed_block(
        self,
        renderer: LatexRenderer,
        ctx: StatelessContext[LatexRenderer],
        block: HeadedBlock,
    ) -> str:
        header = f"\\{block.latex_name}"  # i.e. r"\section"
        if block.latex_name in self._pagebreak_before:
            header = "\\pagebreak\n" + header
        if not block.num:
            header += "*"
        escaped_name = renderer.render_unescapedtext(block.name)
        header += f"{{{escaped_name}}}"  # i.e. r"\section*" + "{Section Name}"
        if block.label:
            header += f"\\label{{{block.label}}}"  # i.e. r"\section*{Section Name}" + r"\label{block_label}"
        return f"{header}\n\n" + renderer.render_blockscope(block.contents)

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
    def _add_renderers(self, handler: CustomRenderDispatch[LatexRenderer]):
        handler.add_custom_inline(Formatted, self._render_formatted)

    def _render_formatted(
        self,
        renderer: LatexRenderer,
        ctx: StatelessContext[LatexRenderer],
        item: Formatted,
    ) -> str:
        data = f"\\{item.format_type}{{"
        data += renderer.render_inlinescope(item.items)
        return data + "}"

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
    def _add_renderers(self, handler: CustomRenderDispatch[LatexRenderer]):
        handler.add_custom_block(DisplayList, self._render_list)
        handler.add_custom_block(DisplayListItem, self._render_list_item)

    def _render_list(
        self,
        renderer: LatexRenderer,
        ctx: StatelessContext[LatexRenderer],
        list: DisplayList,
    ) -> str:
        # TODO indents!
        data = f"\\begin{{{list.mode}}}\n"
        data += renderer.PARAGRAPH_SEP.join(
            renderer.render_block(i) for i in list.items
        )
        return data + f"\n\\end{{{list.mode}}}"

    def _render_list_item(
        self,
        renderer: LatexRenderer,
        ctx: StatelessContext[LatexRenderer],
        list_item: DisplayListItem,
    ) -> str:
        # TODO indents!
        # Put {} after \item so square brackets at the start of render_block don't get swallowed as arguments
        return "\\item{} " + renderer.render_block(list_item.item)

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

    def _add_renderers(self, handler: CustomRenderDispatch[LatexRenderer]):
        handler.add_custom_inline(NamedUrl, self._render_url)

    def _render_url(
        self, renderer: Renderer, ctx: StatelessContext[LatexRenderer], url: NamedUrl
    ) -> str:
        if url.name is None:
            return f"\\url{{{url.url}}}"
        else:
            escaped_url_name = renderer.render_inlinescope(url.name)
            return f"\\href{{{url.url}}}{{{escaped_url_name}}}"

    @stateless
    def url(
        self, ctx: StatelessContext[LatexRenderer], url: str, name: Optional[str] = None
    ) -> Inline:
        return NamedUrl(
            url, name=InlineScope([UnescapedText(name)]) if name is not None else None
        )
