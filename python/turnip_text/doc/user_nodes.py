import abc
from typing import Iterable, Protocol, Sequence, runtime_checkable

from turnip_text import Block, Inline
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
    @abc.abstractmethod
    def child_nodes(self) -> Iterable[Block | Inline] | None:
        """The children of this node, used by the DFS pass to iterate into nodes."""
        ...


class NodePortal(Protocol):
    portal_to: Backref | Sequence[Backref]
