from dataclasses import dataclass
from enum import Enum
from typing import Iterable, Sequence

from turnip_text import Block, Inline, Inlines, Text
from turnip_text.doc.user_nodes import UserNode
from turnip_text.env_plugins import EnvPlugin
from turnip_text.helpers import inline_scope_builder
from typing_extensions import override


# TODO strikethrough? sub/superscript? small caps?
# TODO remove italic and bold
class InlineFormattingType(Enum):
    Italic = 0
    Bold = 1
    Underline = 2
    Emph = 3  # Usually italic
    Strong = 4  # Usually bold
    SingleQuote = 5
    DoubleQuote = 6
    Mono = 7


@dataclass(frozen=True)
class InlineFormatted(UserNode, Inline):
    format_type: InlineFormattingType
    contents: Inlines
    anchor = None

    @override
    def child_nodes(self) -> Iterable[Block | Inline] | None:
        return self.contents


# TODO merge UrlEnv, SubfileEnv, Inline(?)Env into PrimitivesEnvPlugin?
class InlineFormatEnvPlugin(EnvPlugin):
    def _doc_nodes(self) -> Sequence[type[Block] | type[Inline]]:
        return (InlineFormatted,)

    @inline_scope_builder
    @staticmethod
    def italic(inlines: Inlines) -> Inline:
        """Format an inline scope in italics."""
        return InlineFormatted(
            contents=inlines, format_type=InlineFormattingType.Italic
        )

    @inline_scope_builder
    @staticmethod
    def bold(inlines: Inlines) -> Inline:
        return InlineFormatted(contents=inlines, format_type=InlineFormattingType.Bold)

    @inline_scope_builder
    @staticmethod
    def underline(inlines: Inlines) -> Inline:
        return InlineFormatted(
            contents=inlines, format_type=InlineFormattingType.Underline
        )

    @inline_scope_builder
    @staticmethod
    def emph(inlines: Inlines) -> Inline:
        return InlineFormatted(contents=inlines, format_type=InlineFormattingType.Emph)

    @inline_scope_builder
    @staticmethod
    def strong(inlines: Inlines) -> Inline:
        return InlineFormatted(
            contents=inlines, format_type=InlineFormattingType.Strong
        )

    @inline_scope_builder
    @staticmethod
    def mono(inlines: Inlines) -> Inline:
        """Intended for monospaced text, such as code, but never provides syntax highlighting."""
        return InlineFormatted(contents=inlines, format_type=InlineFormattingType.Mono)

    @inline_scope_builder
    @staticmethod
    def squote(inlines: Inlines) -> Inline:
        return InlineFormatted(
            contents=inlines, format_type=InlineFormattingType.SingleQuote
        )

    @inline_scope_builder
    @staticmethod
    def enquote(inlines: Inlines) -> Inline:
        return InlineFormatted(
            contents=inlines, format_type=InlineFormattingType.DoubleQuote
        )
