# e.g. arabic, roman numerals (lower and upper), alphabetic
import abc
import string
from dataclasses import dataclass
from typing import Dict, Iterable, List, Mapping, Protocol, Sequence, Tuple

from turnip_text import UnescapedText


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
            raise RuntimeError(f"Can't represent negative number {num} with roman numerals")

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
class Counter(abc.ABC):
    anchor_id: str
    subcounters: List['Counter']
    numbering: Numbering

    value: int = 0

    def increment(self) -> int:
        self.value += 1
        for c in self.subcounters:
            c.reset()
        return self.value

    def reset(self) -> None:
        self.value = 0
        for c in self.subcounters:
            c.reset()

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
        CounterSet._build_anchor_id_lookup(self.anchor_kind_to_parent_chain, [], self.counter_tree_roots)
    
    @staticmethod
    def _build_anchor_id_lookup(
        lookup: Dict[str, Tuple[Counter, ...]],
        parents: Sequence[Counter],
        cs: List[Counter]
    ) -> None:
        for c in cs:
            if c.anchor_id in lookup:
                raise RuntimeError(f"Counter {c.anchor_id} declared twice")
            chain: Tuple[Counter, ...] = (*parents, c)
            lookup[c.anchor_id] = chain
            CounterSet._build_anchor_id_lookup(
                lookup,
                parents=chain,
                cs=c.subcounters
            )

    def anchor_kinds(self) -> Iterable[str]:
        return self.anchor_kind_to_parent_chain.keys()
    
    def increment_counter(self, anchor_kind: str) -> Tuple[UnescapedText, ...]:
        parent_chain = self.anchor_kind_to_parent_chain[anchor_kind]

        # The one at the end of the chain is the counter for this anchor kind
        parent_chain[-1].increment()

        return tuple(c.numbering[c.value] for c in parent_chain)