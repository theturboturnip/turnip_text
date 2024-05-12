# Autogenerated by generate_pandoc_typestub.py

from typing import Dict, List, Literal, Optional, Tuple

from typing_extensions import overload

class Pandoc:
    def __init__(self, arg0: "Meta", arg1: List["Block"]) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Meta": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> List["Block"]: ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Meta") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: List["Block"]) -> None: ...

class Block:
    pass

class Plain(Block):
    def __init__(self, arg0: List["Inline"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["Inline"]: ...
    def __setitem__(self, index: Literal[0], obj: List["Inline"]) -> None: ...

class Para(Block):
    def __init__(self, arg0: List["Inline"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["Inline"]: ...
    def __setitem__(self, index: Literal[0], obj: List["Inline"]) -> None: ...

class LineBlock(Block):
    def __init__(self, arg0: List[List["Inline"]]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List[List["Inline"]]: ...
    def __setitem__(self, index: Literal[0], obj: List[List["Inline"]]) -> None: ...

class CodeBlock(Block):
    def __init__(self, arg0: "Attr", arg1: "Text") -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Attr": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> "Text": ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Attr") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: "Text") -> None: ...

class RawBlock(Block):
    def __init__(self, arg0: "Format", arg1: "Text") -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Format": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> "Text": ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Format") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: "Text") -> None: ...

class BlockQuote(Block):
    def __init__(self, arg0: List["Block"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["Block"]: ...
    def __setitem__(self, index: Literal[0], obj: List["Block"]) -> None: ...

class OrderedList(Block):
    def __init__(self, arg0: "ListAttributes", arg1: List[List["Block"]]) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "ListAttributes": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> List[List["Block"]]: ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "ListAttributes") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: List[List["Block"]]) -> None: ...

class BulletList(Block):
    def __init__(self, arg0: List[List["Block"]]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List[List["Block"]]: ...
    def __setitem__(self, index: Literal[0], obj: List[List["Block"]]) -> None: ...

class DefinitionList(Block):
    def __init__(
        self, arg0: List[Tuple[List["Inline"], List[List["Block"]]]]
    ) -> None: ...
    def __getitem__(
        self, index: Literal[0]
    ) -> List[Tuple[List["Inline"], List[List["Block"]]]]: ...
    def __setitem__(
        self, index: Literal[0], obj: List[Tuple[List["Inline"], List[List["Block"]]]]
    ) -> None: ...

class Header(Block):
    def __init__(self, arg0: "Int", arg1: "Attr", arg2: List["Inline"]) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Int": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> "Attr": ...
    @overload
    def __getitem__(self, index: Literal[2]) -> List["Inline"]: ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Int") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: "Attr") -> None: ...
    @overload
    def __setitem__(self, index: Literal[2], obj: List["Inline"]) -> None: ...

class HorizontalRule(Block):
    def __init__(
        self,
    ) -> None: ...

class Table(Block):
    def __init__(
        self,
        arg0: "Attr",
        arg1: "Caption",
        arg2: List["ColSpec"],
        arg3: "TableHead",
        arg4: List["TableBody"],
        arg5: "TableFoot",
    ) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Attr": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> "Caption": ...
    @overload
    def __getitem__(self, index: Literal[2]) -> List["ColSpec"]: ...
    @overload
    def __getitem__(self, index: Literal[3]) -> "TableHead": ...
    @overload
    def __getitem__(self, index: Literal[4]) -> List["TableBody"]: ...
    @overload
    def __getitem__(self, index: Literal[5]) -> "TableFoot": ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Attr") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: "Caption") -> None: ...
    @overload
    def __setitem__(self, index: Literal[2], obj: List["ColSpec"]) -> None: ...
    @overload
    def __setitem__(self, index: Literal[3], obj: "TableHead") -> None: ...
    @overload
    def __setitem__(self, index: Literal[4], obj: List["TableBody"]) -> None: ...
    @overload
    def __setitem__(self, index: Literal[5], obj: "TableFoot") -> None: ...

class Div(Block):
    def __init__(self, arg0: "Attr", arg1: List["Block"]) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Attr": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> List["Block"]: ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Attr") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: List["Block"]) -> None: ...

class Null(Block):
    def __init__(
        self,
    ) -> None: ...

Attr = Tuple["Text", List["Text"], List[Tuple["Text", "Text"]]]
Text = str

class TableFoot:
    def __init__(self, arg0: "Attr", arg1: List["Row"]) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Attr": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> List["Row"]: ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Attr") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: List["Row"]) -> None: ...

class Row:
    def __init__(self, arg0: "Attr", arg1: List["Cell"]) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Attr": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> List["Cell"]: ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Attr") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: List["Cell"]) -> None: ...

class Cell:
    def __init__(
        self,
        arg0: "Attr",
        arg1: "Alignment",
        arg2: "RowSpan",
        arg3: "ColSpan",
        arg4: List["Block"],
    ) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Attr": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> "Alignment": ...
    @overload
    def __getitem__(self, index: Literal[2]) -> "RowSpan": ...
    @overload
    def __getitem__(self, index: Literal[3]) -> "ColSpan": ...
    @overload
    def __getitem__(self, index: Literal[4]) -> List["Block"]: ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Attr") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: "Alignment") -> None: ...
    @overload
    def __setitem__(self, index: Literal[2], obj: "RowSpan") -> None: ...
    @overload
    def __setitem__(self, index: Literal[3], obj: "ColSpan") -> None: ...
    @overload
    def __setitem__(self, index: Literal[4], obj: List["Block"]) -> None: ...

class ColSpan:
    def __init__(self, arg0: "Int") -> None: ...
    def __getitem__(self, index: Literal[0]) -> "Int": ...
    def __setitem__(self, index: Literal[0], obj: "Int") -> None: ...

Int = int

class RowSpan:
    def __init__(self, arg0: "Int") -> None: ...
    def __getitem__(self, index: Literal[0]) -> "Int": ...
    def __setitem__(self, index: Literal[0], obj: "Int") -> None: ...

class Alignment:
    pass

class AlignLeft(Alignment):
    def __init__(
        self,
    ) -> None: ...

class AlignRight(Alignment):
    def __init__(
        self,
    ) -> None: ...

class AlignCenter(Alignment):
    def __init__(
        self,
    ) -> None: ...

class AlignDefault(Alignment):
    def __init__(
        self,
    ) -> None: ...

class TableBody:
    def __init__(
        self, arg0: "Attr", arg1: "RowHeadColumns", arg2: List["Row"], arg3: List["Row"]
    ) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Attr": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> "RowHeadColumns": ...
    @overload
    def __getitem__(self, index: Literal[2]) -> List["Row"]: ...
    @overload
    def __getitem__(self, index: Literal[3]) -> List["Row"]: ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Attr") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: "RowHeadColumns") -> None: ...
    @overload
    def __setitem__(self, index: Literal[2], obj: List["Row"]) -> None: ...
    @overload
    def __setitem__(self, index: Literal[3], obj: List["Row"]) -> None: ...

class RowHeadColumns:
    def __init__(self, arg0: "Int") -> None: ...
    def __getitem__(self, index: Literal[0]) -> "Int": ...
    def __setitem__(self, index: Literal[0], obj: "Int") -> None: ...

class TableHead:
    def __init__(self, arg0: "Attr", arg1: List["Row"]) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Attr": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> List["Row"]: ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Attr") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: List["Row"]) -> None: ...

ColSpec = Tuple["Alignment", "ColWidth"]

class ColWidth:
    pass

class ColWidth_(ColWidth):
    def __init__(self, arg0: "Double") -> None: ...
    def __getitem__(self, index: Literal[0]) -> "Double": ...
    def __setitem__(self, index: Literal[0], obj: "Double") -> None: ...

class ColWidthDefault(ColWidth):
    def __init__(
        self,
    ) -> None: ...

Double = float

class Caption:
    def __init__(self, arg0: Optional["ShortCaption"], arg1: List["Block"]) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> Optional["ShortCaption"]: ...
    @overload
    def __getitem__(self, index: Literal[1]) -> List["Block"]: ...
    @overload
    def __setitem__(self, index: Literal[0], obj: Optional["ShortCaption"]) -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: List["Block"]) -> None: ...

ShortCaption = List["Inline"]

class Inline:
    pass

class Str(Inline):
    def __init__(self, arg0: "Text") -> None: ...
    def __getitem__(self, index: Literal[0]) -> "Text": ...
    def __setitem__(self, index: Literal[0], obj: "Text") -> None: ...

class Emph(Inline):
    def __init__(self, arg0: List["Inline"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["Inline"]: ...
    def __setitem__(self, index: Literal[0], obj: List["Inline"]) -> None: ...

class Underline(Inline):
    def __init__(self, arg0: List["Inline"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["Inline"]: ...
    def __setitem__(self, index: Literal[0], obj: List["Inline"]) -> None: ...

class Strong(Inline):
    def __init__(self, arg0: List["Inline"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["Inline"]: ...
    def __setitem__(self, index: Literal[0], obj: List["Inline"]) -> None: ...

class Strikeout(Inline):
    def __init__(self, arg0: List["Inline"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["Inline"]: ...
    def __setitem__(self, index: Literal[0], obj: List["Inline"]) -> None: ...

class Superscript(Inline):
    def __init__(self, arg0: List["Inline"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["Inline"]: ...
    def __setitem__(self, index: Literal[0], obj: List["Inline"]) -> None: ...

class Subscript(Inline):
    def __init__(self, arg0: List["Inline"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["Inline"]: ...
    def __setitem__(self, index: Literal[0], obj: List["Inline"]) -> None: ...

class SmallCaps(Inline):
    def __init__(self, arg0: List["Inline"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["Inline"]: ...
    def __setitem__(self, index: Literal[0], obj: List["Inline"]) -> None: ...

class Quoted(Inline):
    def __init__(self, arg0: "QuoteType", arg1: List["Inline"]) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "QuoteType": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> List["Inline"]: ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "QuoteType") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: List["Inline"]) -> None: ...

class Cite(Inline):
    def __init__(self, arg0: List["Citation"], arg1: List["Inline"]) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> List["Citation"]: ...
    @overload
    def __getitem__(self, index: Literal[1]) -> List["Inline"]: ...
    @overload
    def __setitem__(self, index: Literal[0], obj: List["Citation"]) -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: List["Inline"]) -> None: ...

class Code(Inline):
    def __init__(self, arg0: "Attr", arg1: "Text") -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Attr": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> "Text": ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Attr") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: "Text") -> None: ...

class Space(Inline):
    def __init__(
        self,
    ) -> None: ...

class SoftBreak(Inline):
    def __init__(
        self,
    ) -> None: ...

class LineBreak(Inline):
    def __init__(
        self,
    ) -> None: ...

class Math(Inline):
    def __init__(self, arg0: "MathType", arg1: "Text") -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "MathType": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> "Text": ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "MathType") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: "Text") -> None: ...

class RawInline(Inline):
    def __init__(self, arg0: "Format", arg1: "Text") -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Format": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> "Text": ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Format") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: "Text") -> None: ...

class Link(Inline):
    def __init__(self, arg0: "Attr", arg1: List["Inline"], arg2: "Target") -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Attr": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> List["Inline"]: ...
    @overload
    def __getitem__(self, index: Literal[2]) -> "Target": ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Attr") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: List["Inline"]) -> None: ...
    @overload
    def __setitem__(self, index: Literal[2], obj: "Target") -> None: ...

class Image(Inline):
    def __init__(self, arg0: "Attr", arg1: List["Inline"], arg2: "Target") -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Attr": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> List["Inline"]: ...
    @overload
    def __getitem__(self, index: Literal[2]) -> "Target": ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Attr") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: List["Inline"]) -> None: ...
    @overload
    def __setitem__(self, index: Literal[2], obj: "Target") -> None: ...

class Note(Inline):
    def __init__(self, arg0: List["Block"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["Block"]: ...
    def __setitem__(self, index: Literal[0], obj: List["Block"]) -> None: ...

class Span(Inline):
    def __init__(self, arg0: "Attr", arg1: List["Inline"]) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Attr": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> List["Inline"]: ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Attr") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: List["Inline"]) -> None: ...

Target = Tuple["Text", "Text"]

class Format:
    def __init__(self, arg0: "Text") -> None: ...
    def __getitem__(self, index: Literal[0]) -> "Text": ...
    def __setitem__(self, index: Literal[0], obj: "Text") -> None: ...

class MathType:
    pass

class DisplayMath(MathType):
    def __init__(
        self,
    ) -> None: ...

class InlineMath(MathType):
    def __init__(
        self,
    ) -> None: ...

class Citation:
    def __init__(
        self,
        arg0: "Text",
        arg1: List["Inline"],
        arg2: List["Inline"],
        arg3: "CitationMode",
        arg4: "Int",
        arg5: "Int",
    ) -> None: ...
    @overload
    def __getitem__(self, index: Literal[0]) -> "Text": ...
    @overload
    def __getitem__(self, index: Literal[1]) -> List["Inline"]: ...
    @overload
    def __getitem__(self, index: Literal[2]) -> List["Inline"]: ...
    @overload
    def __getitem__(self, index: Literal[3]) -> "CitationMode": ...
    @overload
    def __getitem__(self, index: Literal[4]) -> "Int": ...
    @overload
    def __getitem__(self, index: Literal[5]) -> "Int": ...
    @overload
    def __setitem__(self, index: Literal[0], obj: "Text") -> None: ...
    @overload
    def __setitem__(self, index: Literal[1], obj: List["Inline"]) -> None: ...
    @overload
    def __setitem__(self, index: Literal[2], obj: List["Inline"]) -> None: ...
    @overload
    def __setitem__(self, index: Literal[3], obj: "CitationMode") -> None: ...
    @overload
    def __setitem__(self, index: Literal[4], obj: "Int") -> None: ...
    @overload
    def __setitem__(self, index: Literal[5], obj: "Int") -> None: ...

class CitationMode:
    pass

class AuthorInText(CitationMode):
    def __init__(
        self,
    ) -> None: ...

class SuppressAuthor(CitationMode):
    def __init__(
        self,
    ) -> None: ...

class NormalCitation(CitationMode):
    def __init__(
        self,
    ) -> None: ...

class QuoteType:
    pass

class SingleQuote(QuoteType):
    def __init__(
        self,
    ) -> None: ...

class DoubleQuote(QuoteType):
    def __init__(
        self,
    ) -> None: ...

ListAttributes = Tuple["Int", "ListNumberStyle", "ListNumberDelim"]

class ListNumberDelim:
    pass

class DefaultDelim(ListNumberDelim):
    def __init__(
        self,
    ) -> None: ...

class Period(ListNumberDelim):
    def __init__(
        self,
    ) -> None: ...

class OneParen(ListNumberDelim):
    def __init__(
        self,
    ) -> None: ...

class TwoParens(ListNumberDelim):
    def __init__(
        self,
    ) -> None: ...

class ListNumberStyle:
    pass

class DefaultStyle(ListNumberStyle):
    def __init__(
        self,
    ) -> None: ...

class Example(ListNumberStyle):
    def __init__(
        self,
    ) -> None: ...

class Decimal(ListNumberStyle):
    def __init__(
        self,
    ) -> None: ...

class LowerRoman(ListNumberStyle):
    def __init__(
        self,
    ) -> None: ...

class UpperRoman(ListNumberStyle):
    def __init__(
        self,
    ) -> None: ...

class LowerAlpha(ListNumberStyle):
    def __init__(
        self,
    ) -> None: ...

class UpperAlpha(ListNumberStyle):
    def __init__(
        self,
    ) -> None: ...

class Meta:
    def __init__(self, arg0: Dict["Text", "MetaValue"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> Dict["Text", "MetaValue"]: ...
    def __setitem__(
        self, index: Literal[0], obj: Dict["Text", "MetaValue"]
    ) -> None: ...

class MetaValue:
    pass

class MetaMap(MetaValue):
    def __init__(self, arg0: Dict["Text", "MetaValue"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> Dict["Text", "MetaValue"]: ...
    def __setitem__(
        self, index: Literal[0], obj: Dict["Text", "MetaValue"]
    ) -> None: ...

class MetaList(MetaValue):
    def __init__(self, arg0: List["MetaValue"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["MetaValue"]: ...
    def __setitem__(self, index: Literal[0], obj: List["MetaValue"]) -> None: ...

class MetaBool(MetaValue):
    def __init__(self, arg0: "Bool") -> None: ...
    def __getitem__(self, index: Literal[0]) -> "Bool": ...
    def __setitem__(self, index: Literal[0], obj: "Bool") -> None: ...

class MetaString(MetaValue):
    def __init__(self, arg0: "Text") -> None: ...
    def __getitem__(self, index: Literal[0]) -> "Text": ...
    def __setitem__(self, index: Literal[0], obj: "Text") -> None: ...

class MetaInlines(MetaValue):
    def __init__(self, arg0: List["Inline"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["Inline"]: ...
    def __setitem__(self, index: Literal[0], obj: List["Inline"]) -> None: ...

class MetaBlocks(MetaValue):
    def __init__(self, arg0: List["Block"]) -> None: ...
    def __getitem__(self, index: Literal[0]) -> List["Block"]: ...
    def __setitem__(self, index: Literal[0], obj: List["Block"]) -> None: ...

Bool = bool