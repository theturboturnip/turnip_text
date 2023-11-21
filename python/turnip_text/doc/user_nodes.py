from dataclasses import dataclass
from typing import Iterable, Protocol, Sequence, Union, runtime_checkable

from turnip_text import Block, BlockScope, DocSegmentHeader, Inline, InlineScope
from turnip_text.doc.anchors import Anchor


class VisitableNode(Protocol):
    contents: Iterable[Block | Inline] | None


@dataclass(frozen=True)
class UserBlock(VisitableNode, Block):
    contents: Iterable[Block | Inline] | None


@dataclass(frozen=True)
class UserAnchorBlock(VisitableNode, Block):
    contents: Iterable[Block | Inline] | None
    anchor: Anchor | None  # Optional field, accessed with getattr, assumed to be None if not present.


@dataclass(frozen=True)
class UserInline(VisitableNode, Inline):
    contents: Iterable[Block | Inline] | None


@dataclass(frozen=True)
class UserAnchorInline(VisitableNode, Inline):
    contents: Iterable[Block | Inline] | None
    anchor: Anchor | None  # Optional field, accessed with getattr, assumed to be None if not present.


@dataclass(frozen=True)
class UserAnchorDocSegmentHeader(DocSegmentHeader):
    contents: Iterable[Block | Inline] | None
    anchor: Anchor | None  # Optional field, accessed with getattr, assumed to be None if not present.
    weight: int
