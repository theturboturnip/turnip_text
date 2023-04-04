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
from turnip_text.renderers.std_plugins import (
    CitationPluginInterface,
    CiteKey,
    FootnotePluginInterface,
    FormatPluginInterface,
    SectionPluginInterface,
)

from .base import RawLatex

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
    format_type: str  # e.g. "emph"
    items: InlineScope


class LatexCitationPlugin(RendererPlugin, CitationPluginInterface):
    _citations: Dict[str, Any]

    def __init__(self) -> None:
        super().__init__()

        # TODO load citations from somewhere
        self._citations = {}

    def _inline_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((Citation, self._render_citation),)

    def _render_citation(self, renderer: Renderer, citation: Citation) -> str:
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


class LatexFootnotePlugin(RendererPlugin, FootnotePluginInterface):
    _footnotes: Dict[str, Block]

    def __init__(self) -> None:
        super().__init__()

        self._footnotes = {}

    def _inline_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((FootnoteAnchor, self._render_footnote_anchor),)

    def _render_footnote_anchor(
        self, renderer: Renderer, footnote: FootnoteAnchor
    ) -> str:
        # TODO - intelligent footnotetext placement using floats?
        rendered_footnotetext = renderer.render_block(self._footnotes[footnote.label])
        return f"\\footnote{{{rendered_footnotetext}}}"

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


class LatexSectionPlugin(RendererPlugin, SectionPluginInterface):
    def _block_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((HeadedBlock, self._render_headed_block),)

    def _render_headed_block(self, renderer: Renderer, block: HeadedBlock) -> str:
        header = f"\\{block.latex_name}"  # i.e. r"\section"
        if block.num:
            header += "*"
        escaped_name = renderer.render_unescapedtext(block.name)
        header += f"{{{escaped_name}}}"  # i.e. r"\section*" + "{Section Name}"
        if block.label:
            header += f"\\label{{{block.label}}}"  # i.e. r"\section*{Section Name}" + r"\label{block_label}"
        return f"{header}\n\n" + renderer.render_blockscope(block.contents)

    def section(
        self, name: str, label: Optional[str] = None, num: bool = True
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

    def subsection(
        self, name: str, label: Optional[str] = None, num: bool = True
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

    def subsubsection(
        self, name: str, label: Optional[str] = None, num: bool = True
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


class LatexFormatPlugin(RendererPlugin, FormatPluginInterface):
    def _inline_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((Formatted, self._render_formatted),)

    def _render_formatted(self, renderer: Renderer, item: Formatted) -> str:
        data = f"\\{item.format_type}{{"
        data += renderer.render_inlinescope(item.items)
        return data + "}"

    @dictify_pure_property
    def emph(self) -> InlineScopeBuilder:
        @inline_scope_builder
        def emph_builder(items: InlineScope) -> Inline:
            return Formatted("emph", items)

        return emph_builder

    OPEN_DQUOTE = RawLatex("``")
    CLOS_DQUOTE = RawLatex("''")

    @dictify_pure_property
    def enquote(self) -> InlineScopeBuilder:
        @inline_scope_builder
        def enquote_builder(items: InlineScope) -> Inline:
            return InlineScope(
                [LatexFormatPlugin.OPEN_DQUOTE]
                + list(items)
                + [LatexFormatPlugin.CLOS_DQUOTE]
            )

        return enquote_builder


class LatexListPlugin(RendererPlugin):
    def _block_handlers(self) -> Iterable[CustomRenderFunc]:
        return (
            (DisplayList, self._render_list),
            (DisplayListItem, self._render_list_item),
        )

    def _render_list(self, renderer: Renderer, list: DisplayList) -> str:
        # TODO indents!
        data = f"\\begin{{{list.mode}}}\n"
        data += renderer.PARAGRAPH_SEP.join(
            renderer.render_block(i) for i in list.items
        )
        return data + f"\n\\end{{{list.mode}}}"

    def _render_list_item(self, renderer: Renderer, list_item: DisplayListItem) -> str:
        # TODO indents!
        return "\\item " + renderer.render_block(list_item.item)

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


class LatexUrlPlugin(RendererPlugin):
    # TODO add dependency on hyperref!!

    def _inline_handlers(self) -> Iterable[CustomRenderFunc]:
        return ((Url, self._render_url),)

    def _render_url(self, renderer: Renderer, url: Url) -> str:
        escaped_url = url.url.replace("#", "\\#")
        return f"\\url{{{escaped_url}}}"

    def url(self, url: str) -> Inline:
        return Url(url)
