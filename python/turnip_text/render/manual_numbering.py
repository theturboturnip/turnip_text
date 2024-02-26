import string
from typing import Protocol

from turnip_text import UnescapedText


class ManualNumbering(Protocol):
    def __getitem__(self, num: int) -> UnescapedText: ...


class BasicManualNumbering(ManualNumbering):
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


class RomanManualNumbering(ManualNumbering):
    upper: bool

    def __init__(self, upper: bool) -> None:
        self.upper = upper

    def __getitem__(self, num: int) -> UnescapedText:
        if num <= 0:
            raise RuntimeError(f"Can't represent {num} with roman numerals")

        s = ""
        for divisor, roman in ROMAN_NUMBER_LOWER:
            s += roman * (num // divisor)
            num = num % divisor
        if self.upper:
            s = s.upper()

        return UnescapedText(s)


class ArabicManualNumbering(ManualNumbering):
    def __getitem__(self, num: int) -> UnescapedText:
        return UnescapedText(str(num))


ARABIC_NUMBERING = ArabicManualNumbering()
LOWER_ROMAN_NUMBERING = RomanManualNumbering(upper=False)
UPPER_ROMAN_NUMBERING = RomanManualNumbering(upper=True)
LOWER_ALPH_NUMBERING = BasicManualNumbering(string.ascii_lowercase)
UPPER_ALPH_NUMBERING = BasicManualNumbering(string.ascii_uppercase)
