# e.g. arabic, roman numerals (lower and upper), alphabetic
import abc
import string
from dataclasses import dataclass
from typing import Dict, Iterable, List, Mapping, Protocol, Sequence, Tuple

from turnip_text import (
    Inline,
    InlineScope,
    UnescapedText,
    coerce_to_inline,
    join_inlines,
)


class Numbering(Protocol):
    def __getitem__(self, num: int) -> UnescapedText:
        ...


class BasicNumbering(Numbering):
    lookup: str

    def __init__(self, lookup: str) -> None:
        self.lookup = lookup

    def __getitem__(self, num: int) -> UnescapedText:
        if num < 1:
            raise RuntimeError(f"Can't represent number {num} - too small")
        if num > len(self.lookup):
            raise RuntimeError(f"Can't represent number {num} - too large")
        return UnescapedText(self.lookup[num - 1])


# Roman numbering based on https://www.geeksforgeeks.org/python-program-to-convert-integer-to-roman/
ROMAN_NUMBER_LOWER = [
    (1000, "m"),
    (900, "cm"),
    (500, "d"),
    (400, "cd"),
    (100, "c"),
    (90, "xc"),
    (50, "l"),
    (40, "xl"),
    (10, "x"),
    (9, "ix"),
    (5, "v"),
    (4, "iv"),
    (1, "i"),
]


class RomanNumbering(Numbering):
    upper: bool

    def __init__(self, upper: bool) -> None:
        self.upper = upper

    def __getitem__(self, num: int) -> UnescapedText:
        if num < 0:
            raise RuntimeError(
                f"Can't represent negative number {num} with roman numerals"
            )

        s = ""
        for divisor, roman in ROMAN_NUMBER_LOWER:
            s += roman * (num // divisor)
            num = num % divisor
        if self.upper:
            s = s.upper()

        return UnescapedText(s)


class ArabicNumbering(Numbering):
    def __getitem__(self, num: int) -> UnescapedText:
        return UnescapedText(str(num))


ARABIC_NUMBERING = ArabicNumbering()
LOWER_ROMAN_NUMBERING = RomanNumbering(upper=False)
UPPER_ROMAN_NUMBERING = RomanNumbering(upper=True)
LOWER_ALPH_NUMBERING = BasicNumbering(string.ascii_lowercase)
UPPER_ALPH_NUMBERING = BasicNumbering(string.ascii_uppercase)

@dataclass
class CounterChainValue:
    # The chain of counters, where element [-1] is the main counter for this value
    # and [-2], [-3]... are parent, grandparent... counters etc.
    # Each element of this tuple is a pair (counter, value).
    parent_counters: Tuple[Tuple["Counter", int], ...]

    def __post_init__(self) -> None:
        if not self.parent_counters:
            raise ValueError("CounterChainValue must have at least one (Counter, value) pair")

    def render(self) -> Inline:
        """Take the last-level counter and use it to render out the whole counter chain.
        
        e.g. if Figures were per-chapter per-section, the chain is ((chapter, X), (section, Y), (figure, Z)).
        Use `figure` to render that chain into e.g. 'Figure X.Y.Z'.
        """
        return self.parent_counters[-1][0].render_counter(self.parent_counters)


# TODO need to rename this because of conflict with collections.Counter
class Counter(abc.ABC):
    anchor_id: str
    numbering: Numbering
    subcounters: List["Counter"]

    value: int = 0

    def __init__(self, anchor_id: str, numbering: Numbering, subcounters: List["Counter"]) -> None:
        super().__init__()
        self.anchor_id = anchor_id
        self.numbering = numbering
        self.subcounters = subcounters

    @abc.abstractmethod
    def render_counter(self, parent_chain: Iterable[Tuple["Counter", int]]) -> Inline:
        raise NotImplementedError()

    def increment(self) -> int:
        self.value += 1
        for c in self.subcounters:
            c.reset()
        return self.value

    def reset(self) -> None:
        self.value = 0
        for c in self.subcounters:
            c.reset()

class BasicCounter(Counter):
    prefix: str

    def __init__(self, anchor_id: str, prefix: str, numbering: Numbering, subcounters: List[Counter]) -> None:
        super().__init__(anchor_id, numbering, subcounters)
        self.prefix = prefix

    def render_counter(self, parent_chain: Iterable[Tuple[Counter, int]]) -> Inline:
        return InlineScope([
            UnescapedText(f"{self.prefix} "),
            join_inlines((c.numbering[v] for c, v in parent_chain), UnescapedText("."))
        ])

class CounterSet:
    # The roots of the tree of labels
    counter_tree_roots: List[Counter]
    # Mapping of Anchor.id to the chain of Counters.
    # e.g. if Heading -> Subheading -> Subsubheading,
    # then anchor_id_lookup[subsubheading] = (HeadingCounter, SubheadingCounter, SubsubheadingCounter)
    anchor_kind_to_parent_chain: Mapping[str, Tuple[Counter, ...]]

    def __init__(self, counter_tree_roots: List[Counter]) -> None:
        self.counter_tree_roots = counter_tree_roots
        self.anchor_kind_to_parent_chain = {}
        CounterSet._build_anchor_id_lookup(
            self.anchor_kind_to_parent_chain, [], self.counter_tree_roots
        )

    @staticmethod
    def _build_anchor_id_lookup(
        lookup: Dict[str, Tuple[Counter, ...]],
        parents: Sequence[Counter],
        cs: List[Counter],
    ) -> None:
        for c in cs:
            if c.anchor_id in lookup:
                raise RuntimeError(f"Counter {c.anchor_id} declared twice")
            chain: Tuple[Counter, ...] = (*parents, c)
            lookup[c.anchor_id] = chain
            CounterSet._build_anchor_id_lookup(lookup, parents=chain, cs=c.subcounters)

    def anchor_kinds(self) -> Iterable[str]:
        return self.anchor_kind_to_parent_chain.keys()

    def increment_counter(self, anchor_kind: str) -> CounterChainValue:
        parent_chain = self.anchor_kind_to_parent_chain[anchor_kind]

        # The one at the end of the chain is the counter for this anchor kind
        parent_chain[-1].increment()

        return CounterChainValue(tuple((c, c.value) for c in parent_chain))
