import abc
from dataclasses import dataclass, field
from typing import Iterable, Protocol, Sequence, Union, runtime_checkable

from turnip_text import Block, BlockScope, Header, Inline, InlineScope
from turnip_text.doc.anchors import Anchor, Backref


@runtime_checkable
class UserNode(Protocol):
    anchor: Anchor | None
    """The anchor which refers to this node. Each node can have at most one Anchor.
    If the node is reachable from the document, the DFS phase will "count" the anchor."""

    # If you use @property here, you're allowed to add a field with the same name
    # and the dataclass will try to use it in the constructor
    # but it won't have a setter.
    # TODO make this return list - or generator? - to be more easily combined with subclassing
    # TODO rename this to 'expand()'? As an easy way to define renderer-independent nodes that don't have content at the start..? ah, but that would technically allow changing the content after a DFS. technically this is fine for e.g. wordcount nodes, which by definition change after the DFS...
    # maybe the correct thing is to render a user node in terms of its child_nodes() if we don't find a renderer for it...
    @abc.abstractmethod
    def child_nodes(self) -> Iterable[Block | Inline] | None:
        """The children of this node, used by the DFS pass to iterate into nodes."""
        ...


class NodePortal(Protocol):
    portal_to: Backref | Sequence[Backref]
