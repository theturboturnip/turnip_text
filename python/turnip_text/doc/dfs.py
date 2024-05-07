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
from turnip_text.doc.std_plugins import DocAnchors
from turnip_text.doc.user_nodes import UserNode

VisitorFilter = Tuple[Type[Any], ...] | Type[Any] | None
VisitorFunc = Callable[[Any], None]


class DocumentDfsPass:
    visitors: List[Tuple[VisitorFilter, VisitorFunc]]

    def __init__(self, visitors: List[Tuple[VisitorFilter, VisitorFunc]]) -> None:
        self.visitors = visitors

    def dfs_over_document(self, document: Document, anchors: DocAnchors) -> None:
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

            if hasattr(node, "portal_to") and node.portal_to:
                if isinstance(node.portal_to, Backref):
                    portal_to = [node.portal_to]
                else:
                    portal_to = node.portal_to
                for backref in reversed(portal_to):
                    anchor, portal_contents = anchors.lookup_backref_float(backref)
                    if anchor in visited_floats:
                        raise ValueError(f"Multiple nodes are portals to {anchor}")
                    if portal_contents:
                        dfs_queue.append(portal_contents)
