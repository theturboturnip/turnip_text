import string
from dataclasses import dataclass
from typing import Generic, Protocol, Sequence, Tuple, TypeVar

from turnip_text import Inline, InlineScope, Text


class ManualNumbering(Protocol):
    def __getitem__(self, num: int) -> str: ...


class BasicManualNumbering(ManualNumbering):
    lookup: Sequence[str]

    def __init__(self, lookup: Sequence[str]) -> None:
        self.lookup = lookup

    def __getitem__(self, num: int) -> str:
        if num < 0:
            raise RuntimeError(f"Can't represent number {num} - too small")
        if num > len(self.lookup):
            raise RuntimeError(f"Can't represent number {num} - too large")
        return self.lookup[num]


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


class RomanManualNumbering(ManualNumbering):
    upper: bool

    def __init__(self, upper: bool) -> None:
        self.upper = upper

    def __getitem__(self, num: int) -> str:
        if num < 0:
            raise RuntimeError(f"Can't represent {num} with roman numerals")
        if num == 0:
            return "0"

        s = ""
        for divisor, roman in ROMAN_NUMBER_LOWER:
            s += roman * (num // divisor)
            num = num % divisor
        if self.upper:
            s = s.upper()

        return s


class ArabicManualNumbering(ManualNumbering):
    def __getitem__(self, num: int) -> str:
        return str(num)


ARABIC_NUMBERING = ArabicManualNumbering()
LOWER_ROMAN_NUMBERING = RomanManualNumbering(upper=False)
UPPER_ROMAN_NUMBERING = RomanManualNumbering(upper=True)
LOWER_ALPH_NUMBERING = BasicManualNumbering("0" + string.ascii_lowercase)
UPPER_ALPH_NUMBERING = BasicManualNumbering("0" + string.ascii_uppercase)


TNumbering = TypeVar("TNumbering", bound=ManualNumbering)


@dataclass
class SimpleCounterFormat(Generic[TNumbering]):
    """
    The formatting (name and numbering style) for a given counter, and how it's combined with other counters.
    """

    name: str
    """The name references use as a prefix e.g. for figures this would be 'Figure' to produce 'Figure 1.2'. Only the name of the last counter in the chain is used."""

    style: TNumbering
    """The style of the numerical counter."""

    postfix_for_child: str = "."
    """When combined with a child counter, what should be placed between this counter and the child? e.g. for 'Figure 1-2' the parent (section) counter would have `postfix_for_child='-'`"""

    postfix_for_end: str = ""
    """If this is the end of the string of counters, what (if anything) should be placed at the end? e.g. for 'Question 1a)' the final counter would have `postfix_for_end=')'`"""

    @classmethod
    def resolve(
        cls,
        # The bound on the SimpleCounterFormat type-argument is sufficient that here we don't care what the concrete type is.
        counters: Sequence[Tuple["SimpleCounterFormat", int]],  # type: ignore[type-arg]
        with_name: bool = True,
    ) -> Text:
        if with_name and counters[-1][0].name:
            c = counters[-1][0].name + " "
        else:
            c = ""
        prev_fmt = None
        for fmt, i in counters:
            if prev_fmt:
                c += prev_fmt.postfix_for_child
            c += fmt.style[i]
            prev_fmt = fmt
        c += fmt.postfix_for_end

        return Text(c)
