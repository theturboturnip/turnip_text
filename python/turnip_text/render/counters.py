from collections import defaultdict
from dataclasses import dataclass
from typing import (
    Any,
    DefaultDict,
    Dict,
    Iterable,
    List,
    Mapping,
    Optional,
    Sequence,
    Set,
    Tuple,
)

from turnip_text.doc.anchors import Anchor


@dataclass
class CounterChainValue:
    # The chain of counters, where element [-1] is the main counter for this value
    # and [-2], [-3]... are parent, grandparent... counters etc.
    # Each element of this tuple is a pair (counter_anchor_id, value).
    parent_counters: Tuple[Tuple[str, int], ...]

    def __post_init__(self) -> None:
        if not self.parent_counters:
            raise ValueError(
                "CounterChainValue must have at least one (counter id, value) pair"
            )

    # def render(self) -> Inline:
    #     """Take the last-level counter and use it to render out the whole counter chain.

    #     e.g. if Figures were per-chapter per-section, the chain is ((chapter, X), (section, Y), (figure, Z)).
    #     Use `figure` to render that chain into e.g. 'Figure X.Y.Z'.
    #     """
    #     return self.parent_counters[-1][0].render_counter(self.parent_counters)


@dataclass
class DocCounter:
    anchor_id: str
    subcounters: List["DocCounter"]

    value: int = 0

    def __init__(
        self,
        anchor_id: str,
        subcounters: List["DocCounter"],
    ) -> None:
        super().__init__()
        self.anchor_id = anchor_id
        self.subcounters = subcounters

    def increment(self) -> int:
        self.value += 1
        for c in self.subcounters:
            c.reset()
        return self.value

    def reset(self) -> None:
        self.value = 0
        for c in self.subcounters:
            c.reset()


# class BasicCounterFormat:
#     prefix: str

#     def __init__(
#         self,
#         anchor_id: str,
#         prefix: str,
#         numbering: ManualNumbering,
#         subcounters: List[Counter],
#     ) -> None:
#         super().__init__(anchor_id, numbering, subcounters)
#         self.prefix = prefix

#     def render_counter(self, parent_chain: Iterable[Tuple[Counter, int]]) -> Inline:
#         return InlineScope(
#             [
#                 UnescapedText(f"{self.prefix} "),
#                 join_inlines(
#                     (c.numbering[v] for c, v in parent_chain), UnescapedText(".")
#                 ),
#             ]
#         )

CounterLink = Tuple[
    Optional[str], str
]  # superior -> subordinate counter kinds. superior = None => top-of-list


CounterHierarchy = Mapping[str, str | List[str] | "CounterHierarchy"]


def build_counter_hierarchy(
    conflicting_links: Iterable[CounterLink],
) -> CounterHierarchy:
    handled_counters: Set[str] = set()
    subordinate_to_superior: Dict[str, Optional[str]] = {}
    superior_to_subordinate: DefaultDict[Optional[str], List[str]] = defaultdict(list)

    # Step 1: remove conflicting links based on FCFS
    for superior, subordinate in conflicting_links:
        assert subordinate is not None
        if subordinate in subordinate_to_superior:
            # We have already defined the superior for this subordinate
            continue
        handled_counters.add(subordinate)
        subordinate_to_superior[subordinate] = superior
        superior_to_subordinate[superior].append(subordinate)
        if superior is not None:
            handled_counters.add(superior)

    # Step 2: detect loops
    def get_chain(subordinate: str) -> List[str]:
        """This function hops through subordinate_to_superior, building a chain of counters.

        If at any point a loop is encountered, ValueError is thrown.

        Returns the chain."""
        chain = []
        curr_cnt = subordinate
        while True:
            chain.append(curr_cnt)
            next_cnt = subordinate_to_superior.get(curr_cnt, None)
            if next_cnt in chain:
                raise ValueError(
                    f"Found loop in counters: {' -> '.join(str(c) for c in chain)} -> {next_cnt}"
                )
            if next_cnt is None:
                return chain
            curr_cnt = next_cnt

    counters_to_check_for_loops = handled_counters.copy()
    while counters_to_check_for_loops:
        chain = get_chain(next(iter(counters_to_check_for_loops)))
        # The last element of the chain should get auto-assigned to None
        if chain[-1] not in superior_to_subordinate[None]:
            superior_to_subordinate[None].append(chain[-1])
        # Everything in this chain has now been checked for loops
        counters_to_check_for_loops.difference_update(chain)

    # TODO any missing but handleable counters should also get auto-assigned to None

    # Now we have the set of direct links with no conflicts, connect them to make a CounterHierarchy
    def recursive_build_counters(subordinates: List[str]) -> CounterHierarchy:
        hierarchy = {}
        for s in subordinates:
            hierarchy[s] = recursive_build_counters(superior_to_subordinate[s])
        return hierarchy

    return recursive_build_counters(superior_to_subordinate[None])


class CounterState:
    # The roots of the tree of labels
    counter_tree_roots: List[DocCounter]
    # Mapping of Anchor.id to the chain of Counters.
    # e.g. if Heading -> Subheading -> Subsubheading,
    # then anchor_id_lookup[subsubheading] = (HeadingCounter, SubheadingCounter, SubsubheadingCounter)
    anchor_kind_to_parent_chain: Mapping[str, Tuple[DocCounter, ...]]
    anchor_counters: Dict[
        Anchor, CounterChainValue
    ]  # Mapping of anchor to <counter value>

    def __init__(self, expected_counter_hierarchy: CounterHierarchy) -> None:
        self.counter_tree_roots = CounterState._build_counter_tree(
            expected_counter_hierarchy
        )
        self.anchor_kind_to_parent_chain = {}
        CounterState._build_anchor_id_lookup(
            self.anchor_kind_to_parent_chain, [], self.counter_tree_roots
        )
        self.anchor_counters = {}

    @staticmethod
    def _build_counter_tree(hierarchy: CounterHierarchy) -> List[DocCounter]:
        ctrs = []
        for parent_counter, child_counters in hierarchy.items():
            if isinstance(child_counters, str):
                ctrs.append(
                    DocCounter(parent_counter, [DocCounter(child_counters, [])])
                )
            elif isinstance(child_counters, dict):
                ctrs.append(
                    DocCounter(
                        parent_counter, CounterState._build_counter_tree(child_counters)
                    )
                )
            else:
                ctrs.append(
                    DocCounter(
                        parent_counter,
                        [DocCounter(child, []) for child in child_counters],
                    )
                )
        return ctrs

    @staticmethod
    def _build_anchor_id_lookup(
        lookup: Dict[str, Tuple[DocCounter, ...]],
        parents: Sequence[DocCounter],
        cs: List[DocCounter],
    ) -> None:
        for c in cs:
            if c.anchor_id in lookup:
                raise RuntimeError(f"Counter {c.anchor_id} declared twice")
            chain: Tuple[DocCounter, ...] = (*parents, c)
            lookup[c.anchor_id] = chain
            CounterState._build_anchor_id_lookup(
                lookup, parents=chain, cs=c.subcounters
            )

    def anchor_kinds(self) -> Iterable[str]:
        return self.anchor_kind_to_parent_chain.keys()

    def count_anchor(self, anchor: Anchor) -> None:
        if anchor.kind not in self.anchor_kind_to_parent_chain:
            raise ValueError(f"Unknown counter kind '{anchor.kind}'")
        parent_chain = self.anchor_kind_to_parent_chain[anchor.kind]

        # The one at the end of the chain is the counter for this anchor kind
        parent_chain[-1].increment()

        self.anchor_counters[anchor] = CounterChainValue(
            tuple((c.anchor_id, c.value) for c in parent_chain)
        )

    def count_anchor_if_present(self, node: Any) -> None:
        """To be used in DFS visitor passes for renderers that directly use CounterSet for counting."""
        # Counter pass
        anchor = getattr(node, "anchor", None)
        if isinstance(anchor, Anchor):
            self.count_anchor(anchor)
