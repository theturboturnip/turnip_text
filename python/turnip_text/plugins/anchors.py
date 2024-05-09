import re
from collections import defaultdict
from typing import Callable, Dict, Optional, Tuple

from turnip_text import Block
from turnip_text.doc.anchors import Anchor, Backref
from turnip_text.env_plugins import DocEnv, EnvPlugin, in_doc


class StdAnchorPlugin(EnvPlugin):
    """Plugin responsible for keeping track of all the anchors in a document.

    Has enough information to convert a Backref to the Anchor that it refers to (inferring the kind)
    and retrieve information associated with the anchor.
    Allows document code to create anchors with `register_new_anchor()` or `register_new_anchor_with_float()`.
    Any backref can be converted to an Anchor (usually for rendering purposes) with `lookup_backref()`.
    The data associated with an Anchor in `register_new_anchor_with_float()` can be retrieved with an Anchor `lookup_anchor_float()` or a Backref to that Anchor `lookup_backref_float()`.

    Anchors can be created without knowing their ID, at which point this will generate an ID from a monotonic per-kind counter.
    To avoid overlap with user-defined IDs, user-defined IDs must contain at least one alphabetic latin character (upper or lowercase).

    This is a EnvPlugin so that it can use the @in_doc annotation to avoid creating new anchors after the document is frozen.
    """

    #

    # This can be used by all document code to create backrefs, optionally with custom labels.
    backref = Backref

    _anchor_kind_counters: Dict[str, int]
    _anchor_id_to_possible_kinds: Dict[str, Dict[str, Anchor]]
    _anchored_floats: Dict[Anchor, Block]  # TODO rename floating_space

    # Anchor IDs, if they're user-defined, they must be
    _VALID_USER_ANCHOR_ID_REGEX = re.compile(r"\w*[a-zA-Z]\w*")

    def __init__(self) -> None:
        self._anchor_kind_counters = defaultdict(lambda: 1)
        self._anchor_id_to_possible_kinds = defaultdict(dict)
        self._anchored_floats = {}

    @in_doc
    def register_new_anchor(
        self, doc_env: DocEnv, kind: str, id: Optional[str]
    ) -> Anchor:
        """
        When inside the document, create a new anchor.
        """
        if id is None:
            id = str(self._anchor_kind_counters[kind])
        else:
            # Guarantee no overlap with auto-generated anchor IDs
            assert self._VALID_USER_ANCHOR_ID_REGEX.match(
                id
            ), "User-defined anchor IDs must have at least one alphabetic character"

        if self._anchor_id_to_possible_kinds[id].get(kind) is not None:
            raise ValueError(
                f"Tried to register anchor kind={kind}, id={id} when it already existed"
            )

        l = Anchor(
            kind=kind,
            id=id,
        )
        self._anchor_kind_counters[kind] += 1
        self._anchor_id_to_possible_kinds[id][kind] = l
        return l

    def register_new_anchor_with_float(
        self,
        kind: str,
        id: Optional[str],
        float_gen: Callable[[Anchor], Block],
    ) -> Anchor:
        a = self.register_new_anchor(kind, id)
        self._anchored_floats[a] = float_gen(a)
        return a

    def lookup_backref(self, backref: Backref) -> Anchor:
        """
        Should be called by renderers to resolve a backref into an anchor.
        The renderer can then retrieve the counters for the anchor.
        """

        if backref.id not in self._anchor_id_to_possible_kinds:
            raise ValueError(
                f"Backref {backref} refers to an ID '{backref.id}' with no anchor!"
            )

        possible_kinds = self._anchor_id_to_possible_kinds[backref.id]

        if backref.kind is None:
            if len(possible_kinds) != 1:
                raise ValueError(
                    f"Backref {backref} doesn't specify the kind of anchor it's referring to, and there are multiple with that ID: {possible_kinds}"
                )
            only_possible_anchor = next(iter(possible_kinds.values()))
            return only_possible_anchor
        else:
            if backref.kind not in possible_kinds:
                raise ValueError(
                    f"Backref {backref} specifies an anchor of kind {backref.kind}, which doesn't exist for ID {backref.id}: {possible_kinds}"
                )
            return possible_kinds[backref.kind]

    def lookup_anchor_float(self, anchor: Anchor) -> Optional[Block]:
        return self._anchored_floats.get(anchor)

    def lookup_backref_float(self, backref: Backref) -> Tuple[Anchor, Optional[Block]]:
        a = self.lookup_backref(backref)
        return a, self._anchored_floats.get(a)
