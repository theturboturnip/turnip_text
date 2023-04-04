import uuid
from dataclasses import dataclass
from typing import Any, Dict, Iterable, List, Optional, Tuple, Union, cast

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
    label: Optional[str] = None
    num: bool = True


@dataclass(frozen=True)
class Citation(Inline):
    # List of (label, note?)
    labels: List[CiteKeyWithOptionNote]


@dataclass(frozen=True)
class Url(Inline):
    url: str


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
    format_type: str  # e.g. "*", "**"
    items: InlineScope


class MarkdownCitationPlugin(RendererPlugin, CitationPluginInterface):
    _citations: Dict[str, Any]

    def __init__(self) -> None:
        super().__init__()

        # TODO load citations from somewhere
        self._citations = {}

    def _inline_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((Citation, self._render_citation),)

    def _render_citation(self, renderer: Renderer, citation: Citation) -> str:
        raise NotImplementedError("_render_citation")

    def cite(self, *labels: Union[str, Tuple[str, str]]) -> Inline:
        # Convert ["label"] to [("label", None)] so Citation has a consistent format
        adapted_labels = [
            (label, None)
            if isinstance(label, str)
            else (label[0], UnescapedText(label[1]))
            for label in labels
        ]
        return Citation(adapted_labels)

    # TODO make this output \citeauthor
    def citeauthor(self, label: str) -> Inline:
        return Citation([(label, None)])


class MarkdownFootnotePlugin(RendererPlugin, FootnotePluginInterface):
    _footnotes: Dict[str, Block]

    def __init__(self) -> None:
        super().__init__()

        self._footnotes = {}

    def _inline_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((FootnoteAnchor, self._render_footnote_anchor),)

    def _render_footnote_anchor(
        self, renderer: Renderer, footnote: FootnoteAnchor
    ) -> str:
        raise NotImplementedError("_render_footnote_anchor")

    @dictify_pure_property
    def footnote(self) -> InlineScopeBuilder:
        @inline_scope_builder
        def footnote_builder(contents: InlineScope) -> Inline:
            label = str(uuid.uuid4())
            self._footnotes[label] = Paragraph([Sentence([contents])])
            return FootnoteAnchor(label)

        return footnote_builder

    def footnote_ref(self, label: str) -> Inline:
        return FootnoteAnchor(label)

    def footnote_text(self, label: str) -> BlockScopeBuilder:
        # Return a callable which is invoked with the contents of the following inline scope
        # Example usage:
        # [footnote_text("label")]{text}
        # equivalent to
        # [footnote_text("label")(r"text")]
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            self._footnotes[label] = contents
            return None

        return handle_block_contents


class MarkdownSectionPlugin(RendererPlugin, SectionPluginInterface):
    def _block_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((HeadedBlock, self._render_headed_block),)

    def _render_headed_block(self, renderer: Renderer, block: HeadedBlock) -> str:
        raise NotImplementedError("_render_headed_block")

    def section(
        self, name: str, label: Optional[str] = None, num: bool = True
    ) -> BlockScopeBuilder:
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            return HeadedBlock(
                level=1,
                name=UnescapedText(name),
                label=label,
                num=num,
                contents=contents,
            )

        return handle_block_contents

    def subsection(
        self, name: str, label: Optional[str] = None, num: bool = True
    ) -> BlockScopeBuilder:
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            return HeadedBlock(
                level=2,
                name=UnescapedText(name),
                label=label,
                num=num,
                contents=contents,
            )

        return handle_block_contents

    def subsubsection(
        self, name: str, label: Optional[str] = None, num: bool = True
    ) -> BlockScopeBuilder:
        @block_scope_builder
        def handle_block_contents(contents: BlockScope) -> Block:
            return HeadedBlock(
                level=3,
                name=UnescapedText(name),
                label=label,
                num=num,
                contents=contents,
            )

        return handle_block_contents


class MarkdownFormatPlugin(RendererPlugin, FormatPluginInterface):
    def _inline_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((Formatted, self._render_formatted),)

    def _render_formatted(self, renderer: Renderer, item: Formatted) -> str:
        data = (
            item.format_type
            + renderer.render_inlinescope(item.items)
            + item.format_type
        )
        return data + "}"

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
        return (
            (DisplayList, self._render_list),
            (DisplayListItem, self._render_list_item),
        )

    def _render_list(self, renderer: Renderer, list: DisplayList) -> str:
        # TODO indents!
        raise NotImplementedError("_render_list")

    def _render_list_item(self, renderer: Renderer, list_item: DisplayListItem) -> str:
        # TODO indents!
        raise NotImplementedError("_render_list_item")

    @dictify_pure_property
    def enumerate(self) -> BlockScopeBuilder:
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

    @dictify_pure_property
    def itemize(self) -> BlockScopeBuilder:
        @block_scope_builder
        def itemize_builder(contents: BlockScope) -> Block:
            items = list(contents)
            if any(not isinstance(x, DisplayListItem) for x in items):
                raise TypeError(
                    f"Found blocks in this list that were not list [item]s!"
                )
            return DisplayList(mode="itemize", items=cast(List[DisplayListItem], items))

        return itemize_builder

    @dictify_pure_property
    def item(self) -> BlockScopeBuilder:
        @block_scope_builder
        def item_builder(block_scope: BlockScope) -> Block:
            return DisplayListItem(block_scope)

        return item_builder


class MarkdownUrlPlugin(RendererPlugin):
    # TODO add dependency on hyperref!!

    def _inline_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((Url, self._render_url),)

    def _render_url(self, renderer: Renderer, url: Url) -> str:
        raise NotImplementedError("_render_url")

    def url(self, url: str) -> Inline:
        return Url(url)
