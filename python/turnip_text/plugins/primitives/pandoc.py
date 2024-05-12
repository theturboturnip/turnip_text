from dataclasses import dataclass
from typing import Any, Sequence

import turnip_text.render.pandoc.pandoc_types as pan
from turnip_text import Block, Header, Inline, Raw
from turnip_text.build_system import BuildSystem
from turnip_text.helpers import NullRawBuilder, UserRawScopeBuilder
from turnip_text.plugins.primitives import PageBreak, PrimitivesPlugin
from turnip_text.render.pandoc import PandocPlugin, PandocSetup, null_attr


@dataclass(frozen=True)
class PandocRaw(Inline):
    lang: str
    data: str


class PandocRawBuilder(UserRawScopeBuilder):
    lang: str

    def __init__(self, lang: str) -> None:
        super().__init__()
        self.lang = lang

    def build_from_raw(self, r: Raw) -> Header | Block | Inline | None:
        return PandocRaw(self.lang, r.data)


class PandocPrimitivesPlugin(PandocPlugin, PrimitivesPlugin):
    """
    Pandoc primitives plugin.

    Supports raw in various formats when prefixed with 'pandoc-'

    e.g. `[raw("pandoc-latex")]#{\\newcommand}#`

    Does not support page breaks.
    """

    def raw(self, lang: str, **kwargs: Any) -> UserRawScopeBuilder:
        if lang.startswith("pandoc-"):
            lang = lang.removeprefix("pandoc-")
            return PandocRawBuilder(lang)
        else:
            return NullRawBuilder()

    def _doc_nodes(self) -> Sequence[type[Block] | type[Inline] | type[Header]]:
        return list(super()._doc_nodes()) + [PandocRaw]

    def _register(self, build_sys: BuildSystem, setup: PandocSetup) -> None:
        # pandoc doesn't support page breaks
        # FUTURE could insert format-specific Raw blocks to support it - Latex, Markdown/HTML, DOCX, ODT
        setup.makers.register_block(
            PageBreak, lambda pb, r, f: pan.Div(null_attr(), [])
        )
        setup.makers.register_inline(
            PandocRaw,
            lambda raw, renderer, fmt: pan.RawInline(pan.Format(raw.lang), raw.data),
        )
