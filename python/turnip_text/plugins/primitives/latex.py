from dataclasses import dataclass
from typing import Any, List, Literal, Sequence, Tuple, Type
from turnip_text.env_plugins import VisitorFilter, VisitorFunc
from typing_extensions import override

from turnip_text import Block, Header, Inline, Raw
from turnip_text.build_system import BuildSystem
from turnip_text.doc.user_nodes import UserNode
from turnip_text.env_plugins import FmtEnv
from turnip_text.helpers import (
    NullRawBuilder,
    PassthroughRawBuilder,
    UserRawScopeBuilder,
)
from turnip_text.plugins.primitives import PageBreak, PrimitivesPlugin
from turnip_text.render.latex.renderer import LatexPreamblePoint, LatexRenderer
from turnip_text.render.latex.setup import LatexPlugin, LatexSetup


@dataclass(frozen=True)
class PreambleRaw(UserNode, Inline):
    point: LatexPreamblePoint
    data: str
    anchor: None = None

    @override
    def child_nodes(self) -> None:
        return None


class PreambleRawBuilder(UserRawScopeBuilder):
    """Raw scope builder that passes through whatever argument it's given to the preamble_bits"""
    point: LatexPreamblePoint

    def __init__(self, point: LatexPreamblePoint) -> None:
        super().__init__()
        self.point = point

    def build_from_raw(self, raw: Raw) -> PreambleRaw:
        return PreambleRaw(
            self.point,
            raw.data
        )


class LatexPrimitivesPlugin(LatexPlugin, PrimitivesPlugin):
    preamble_bits: List[PreambleRaw]

    def __init__(self) -> None:
        super().__init__()
        self.preamble_bits = []

    def raw(self, lang: str, preamble: bool | Literal['content'] | Literal['code'] = False, **kwargs: Any) -> UserRawScopeBuilder:
        lang = lang.lower().strip()
        if lang in ["tex", "latex"]:
            if preamble == "content":
                return PreambleRawBuilder(LatexPreamblePoint.CONTENT)
            elif preamble == "code" or preamble:
                return PreambleRawBuilder(LatexPreamblePoint.CODE)
            else:
                return PassthroughRawBuilder()
        else:
            return NullRawBuilder()

    def _register(self, build_sys: BuildSystem, setup: LatexSetup) -> None:
        setup.add_preamble_section(
            lambda r: r.emit_raw("\n".join(bit.data for bit in self.preamble_bits if bit.point == LatexPreamblePoint.CODE)),
            point = LatexPreamblePoint.CODE,
        )
        setup.add_preamble_section(
            lambda r: r.emit_raw("\n".join(bit.data for bit in self.preamble_bits if bit.point == LatexPreamblePoint.CONTENT)),
            point = LatexPreamblePoint.CONTENT,
        )
        setup.emitter.register_block_or_inline(PageBreak, self._emit_page_break)
        setup.emitter.register_block_or_inline(PreambleRaw, lambda _, __, ___: None)

    def _doc_nodes(self) -> Sequence[Type[Block | Inline | Header]]:
        return list(super()._doc_nodes()) + [PreambleRaw]

    def _make_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]]:
        return [
            (PreambleRaw, lambda pr: self.preamble_bits.append(pr))
        ]

    def _emit_page_break(
        self, pb: PageBreak, renderer: LatexRenderer, fmt: FmtEnv
    ) -> None:
        if pb.double:
            renderer.emit_macro("cleardoublepage")
        else:
            renderer.emit_macro("clearpage")
