from typing import Any, Callable, Iterable, List, Set, Tuple, Type

from turnip_text import (
    Block,
    BlockScope,
    DocSegment,
    Document,
    Header,
    Inline,
    InlineScope,
    Paragraph,
)
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.doc.user_nodes import UserNode
from turnip_text.env_plugins import AnchorEnv, VisitorFilter, VisitorFunc


class DocumentDfsPass:
    visitors: List[Tuple[VisitorFilter, VisitorFunc]]

    def __init__(self, visitors: List[Tuple[VisitorFilter, VisitorFunc]]) -> None:
        self.visitors = visitors

    def dfs_over_document(self, document: Document, anchors: AnchorEnv) -> None:
        # Floats are parsed when their portals are encountered
        dfs_queue: List[Block | Inline | DocSegment | Header] = []
        dfs_queue.extend(reversed((document.contents, *document.segments)))
        visited_floats: Set[Anchor] = set()
        while dfs_queue:
            node = dfs_queue.pop()

            # Visit the node
            for v_type, v_f in self.visitors:
                if v_type is None or isinstance(node, v_type):
                    v_f(node)

            # Extract children as a reversed iterator.
            # reversed is important because we pop the last thing in the queue off first.
            children: Iterable[Block | Inline | DocSegment | Header] | None = None
            if isinstance(node, (BlockScope, InlineScope)):
                children = reversed(tuple(node))
            elif isinstance(node, DocSegment):
                children = reversed((node.header, node.contents, *node.subsegments))
            elif isinstance(node, Paragraph):
                inls: List[Inline] = []
                for s in reversed(list(node)):
                    inls.extend(reversed(list(s)))
                children = inls
            elif node is None:
                children = None
            elif isinstance(node, UserNode):
                contents = node.child_nodes()
                children = reversed(list(contents)) if contents is not None else None
            if children:
                dfs_queue.extend(children)

            # TODO would it be possible for the dfs_queue to contain Backref (usually emitted from node.child_nods())
            # instead of special-casing portals and creating another class user nodes have to inherit from?
            # Not exactly Backref, but a PortalTo class could be created instead.
            if hasattr(node, "portal_to") and node.portal_to:
                if isinstance(node.portal_to, Backref):
                    portal_to = [node.portal_to]
                else:
                    portal_to = node.portal_to
                for backref in reversed(portal_to):
                    anchor, portal_contents = anchors.lookup_backref_float(backref)
                    # Right now this doesn't work, because we don't insert() in visited_floats.
                    # That's actually OK! because right now our documents do not put countable things inside repeated floats.
                    # TODO: We should probably enforce floats cannot have countable things, and then we can remove this safely. This implies some kind of "document purity" for nodes that can be repeated ad infinitum...
                    # Better solution: allow floats to have countable things except throw a warning or an error if the same anchor is found twice attached to the same class instance. Floats are allowed to repeat, anchors are not (or repeat anchors are ignored/undefined behaviour)
                    if anchor in visited_floats:
                        raise ValueError(f"Multiple nodes are portals to {anchor}")
                    if portal_contents:
                        dfs_queue.append(portal_contents)
