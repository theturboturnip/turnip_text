import pytest

from turnip_text import *


class CustomInline:
    is_inline: bool = True


class CustomBlock:
    is_block: bool = True


class CustomHeader:
    is_header: bool = True
    weight: int = 0

    def __init__(self, weight: int = 0) -> None:
        self.weight = weight


# Paragraph
def test_paragraph_can_hold_sentences():
    Paragraph([Sentence([]), Sentence([]), Sentence([])])


def test_paragraph_can_append_sentences():
    p = Paragraph([])
    p.append_sentence(Sentence([]))


def test_paragraph_must_only_have_sentences():
    filter = r"instance of Sentence, but it wasn't"
    with pytest.raises(TypeError, match=filter):
        Paragraph([Sentence([]), None, Sentence([])])
    with pytest.raises(TypeError, match=filter):
        Paragraph([Sentence([]), 1, Sentence([])])
    with pytest.raises(TypeError, match=filter):
        Paragraph([Sentence([]), "blah", Sentence([])])
    with pytest.raises(TypeError, match=filter):
        Paragraph([Sentence([]), object(), Sentence([])])


def test_paragraph_must_only_append_sentences():
    # This error is in PyO3's generated harness, because the Rust code expects specific types.
    filter = r"cannot be converted to 'Sentence'"
    p = Paragraph([])
    with pytest.raises(TypeError, match=filter):
        p.append_sentence(None)
    with pytest.raises(TypeError, match=filter):
        p.append_sentence(1)
    with pytest.raises(TypeError, match=filter):
        p.append_sentence("blah")
    with pytest.raises(TypeError, match=filter):
        p.append_sentence(object())


# Document
def test_document_can_hold_docsegments():
    Document(
        contents=BlockScope([]),
        segments=[
            DocSegment(header=CustomHeader(), contents=BlockScope([]), subsegments=[]),
            DocSegment(header=CustomHeader(), contents=BlockScope([]), subsegments=[]),
            DocSegment(header=CustomHeader(), contents=BlockScope([]), subsegments=[]),
        ],
    )


def test_document_can_append_headers():
    d = Document(contents=BlockScope([]), segments=[])
    d.append_header(CustomHeader())


def test_document_can_insert_headers():
    d = Document(
        contents=BlockScope([]),
        segments=[DocSegment(CustomHeader(), BlockScope([]), [])],
    )
    d.insert_header(0, CustomHeader())


def test_document_must_only_have_docsegments():
    filter = r"cannot be converted to 'DocSegment'"
    with pytest.raises(TypeError, match=filter):
        Document(
            contents=BlockScope([]),
            segments=[
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
                None,
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
            ],
        )
    with pytest.raises(TypeError, match=filter):
        Document(
            contents=BlockScope([]),
            segments=[
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
                1,
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
            ],
        )
    with pytest.raises(TypeError, match=filter):
        Document(
            contents=BlockScope([]),
            segments=[
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
                "blah",
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
            ],
        )
    with pytest.raises(TypeError, match=filter):
        Document(
            contents=BlockScope([]),
            segments=[
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
                object(),
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
            ],
        )


def test_document_must_only_append_headers():
    filter = r"instance of Header, but it didn't have the properties.*is_header.*weight"
    p = Document(contents=BlockScope([]), segments=[])
    with pytest.raises(TypeError, match=filter):
        p.append_header(None)
    with pytest.raises(TypeError, match=filter):
        p.append_header(1)
    with pytest.raises(TypeError, match=filter):
        p.append_header("blah")
    with pytest.raises(TypeError, match=filter):
        p.append_header(object())


def test_document_must_only_insert_headers():
    filter = r"instance of Header, but it didn't have the properties.*is_header.*weight"
    p = Document(
        contents=BlockScope([]),
        segments=[DocSegment(CustomHeader(), BlockScope([]), [])],
    )
    with pytest.raises(TypeError, match=filter):
        p.insert_header(0, None)
    with pytest.raises(TypeError, match=filter):
        p.insert_header(0, 1)
    with pytest.raises(TypeError, match=filter):
        p.insert_header(0, "blah")
    with pytest.raises(TypeError, match=filter):
        p.insert_header(0, object())


# DocSegment
def test_docsegment_must_hold_header():
    # Test an object without (is_header) or (weight)
    with pytest.raises(
        TypeError,
        match=f"instance of Header, but it didn't have the properties.*is_header.*weight",
    ):
        DocSegment(header=object(), contents=BlockScope([]), subsegments=[])

    # Test objects with (is_header) but without a (weight) Rust can handle (i.e. that fits in 64bits)
    class IsHeaderWithNoWeight:
        is_header = True

    with pytest.raises(
        TypeError,
        match=f"instance of Header, and it had.*is_header.*but it didn't have.*weight",
    ):
        DocSegment(
            header=IsHeaderWithNoWeight(), contents=BlockScope([]), subsegments=[]
        )

    class IsHeaderWithTooPositiveWeight:
        is_header = True
        weight = (
            9223372036854775808  # This is an integer but it doesn't fit in Rust i64
        )

    class IsHeaderWithTooNegativeWeight:
        is_header = True
        weight = (
            -9223372036854775809
        )  # This is an integer but it doesn't fit in Rust i64

    with pytest.raises(
        TypeError,
        match=f"instance of Header, and it had.*is_header.*but it didn't have.*weight",
    ):
        DocSegment(
            header=IsHeaderWithTooPositiveWeight(),
            contents=BlockScope([]),
            subsegments=[],
        )

    with pytest.raises(
        TypeError,
        match=f"instance of Header, and it had.*is_header.*but it didn't have.*weight",
    ):
        DocSegment(
            header=IsHeaderWithTooNegativeWeight(),
            contents=BlockScope([]),
            subsegments=[],
        )

    # Test objects with (weight) but no (is_header)
    class WeightButNoIsHeader:
        weight = 0

    with pytest.raises(
        TypeError,
        match=f"instance of Header, and it had.*weight.*but it didn't have.*is_header",
    ):
        DocSegment(
            header=WeightButNoIsHeader(), contents=BlockScope([]), subsegments=[]
        )


def test_docsegment_can_hold_docsegments():
    DocSegment(
        header=CustomHeader(weight=-1),
        contents=BlockScope([]),
        subsegments=[
            DocSegment(header=CustomHeader(), contents=BlockScope([]), subsegments=[]),
            DocSegment(header=CustomHeader(), contents=BlockScope([]), subsegments=[]),
            DocSegment(header=CustomHeader(), contents=BlockScope([]), subsegments=[]),
        ],
    )


def test_docsegment_can_append_headers():
    d = DocSegment(
        header=CustomHeader(weight=-1),
        contents=BlockScope([]),
        subsegments=[],
    )
    d.append_header(CustomHeader())


def test_docsegment_can_insert_headers():
    d = DocSegment(
        header=CustomHeader(weight=-1),
        contents=BlockScope([]),
        subsegments=[
            DocSegment(header=CustomHeader(), contents=BlockScope([]), subsegments=[]),
            DocSegment(header=CustomHeader(), contents=BlockScope([]), subsegments=[]),
            DocSegment(header=CustomHeader(), contents=BlockScope([]), subsegments=[]),
        ],
    )
    d.insert_header(2, CustomHeader())


def test_docsegment_must_only_have_docsegments():
    filter = r"cannot be converted to 'DocSegment'"
    with pytest.raises(TypeError, match=filter):
        DocSegment(
            header=CustomHeader(weight=-1),
            contents=BlockScope([]),
            subsegments=[
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
                None,
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
            ],
        )
    with pytest.raises(TypeError, match=filter):
        DocSegment(
            header=CustomHeader(weight=-1),
            contents=BlockScope([]),
            subsegments=[
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
                1,
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
            ],
        )
    with pytest.raises(TypeError, match=filter):
        DocSegment(
            header=CustomHeader(weight=-1),
            contents=BlockScope([]),
            subsegments=[
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
                "blah",
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
            ],
        )
    with pytest.raises(TypeError, match=filter):
        DocSegment(
            header=CustomHeader(weight=-1),
            contents=BlockScope([]),
            subsegments=[
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
                object(),
                DocSegment(
                    header=CustomHeader(), contents=BlockScope([]), subsegments=[]
                ),
            ],
        )


def test_docsegment_must_only_append_headers():
    filter = r"instance of Header, but it didn't have the properties.*is_header.*weight"
    p = DocSegment(header=CustomHeader(), contents=BlockScope([]), subsegments=[])
    with pytest.raises(TypeError, match=filter):
        p.append_header(None)
    with pytest.raises(TypeError, match=filter):
        p.append_header(1)
    with pytest.raises(TypeError, match=filter):
        p.append_header("blah")
    with pytest.raises(TypeError, match=filter):
        p.append_header(object())


def test_docsegment_must_only_insert_headers():
    filter = r"instance of Header, but it didn't have the properties.*is_header.*weight"
    p = DocSegment(header=CustomHeader(), contents=BlockScope([]), subsegments=[])
    with pytest.raises(TypeError, match=filter):
        p.insert_header(1, None)
    with pytest.raises(TypeError, match=filter):
        p.insert_header(1, 1)
    with pytest.raises(TypeError, match=filter):
        p.insert_header(1, "blah")
    with pytest.raises(TypeError, match=filter):
        p.insert_header(1, object())


# BlockScope
def test_block_scope_can_hold_blocks():
    BlockScope([CustomBlock(), CustomBlock(), CustomBlock()])


def test_block_scope_can_append_blocks():
    scope = BlockScope([])
    scope.append_block(CustomBlock())
    scope.append_block(Paragraph([]))
    scope.append_block(BlockScope([]))


def test_block_scope_must_only_have_blocks():
    filter = r"instance of Block, but it didn't have property is_block=True"

    with pytest.raises(TypeError, match=filter):
        BlockScope([CustomBlock(), None, CustomBlock()])
    with pytest.raises(TypeError, match=filter):
        BlockScope([CustomBlock(), 1, CustomBlock()])
    with pytest.raises(TypeError, match=filter):
        BlockScope([CustomBlock(), "blah", CustomBlock()])
    with pytest.raises(TypeError, match=filter):
        BlockScope([CustomBlock(), CustomInline(), CustomBlock()])


def test_block_scope_must_only_append_blocks():
    filter = r"instance of Block, but it didn't have property is_block=True"
    bs = BlockScope([])
    with pytest.raises(TypeError, match=filter):
        bs.append_block(None)
    with pytest.raises(TypeError, match=filter):
        bs.append_block(1)
    with pytest.raises(TypeError, match=filter):
        bs.append_block("blah")
    with pytest.raises(TypeError, match=filter):
        bs.append_block(CustomInline())


# InlineScope
def test_inline_scope_can_hold_inlines():
    InlineScope([CustomInline(), CustomInline(), CustomInline()])


def test_inline_scope_can_append_inlines():
    scope = InlineScope([])
    scope.append_inline(CustomInline())
    scope.append_inline(Text(""))
    scope.append_inline(Raw(""))
    scope.append_inline(InlineScope([]))


def test_inline_scope_must_only_have_inlines():
    filter = r"instance of Inline, but it didn't have property is_inline=True"

    with pytest.raises(TypeError, match=filter):
        InlineScope([CustomInline(), None, CustomInline()])
    with pytest.raises(TypeError, match=filter):
        InlineScope([CustomInline(), 1, CustomInline()])
    with pytest.raises(TypeError, match=filter):
        InlineScope([CustomInline(), "blah", CustomInline()])
    with pytest.raises(TypeError, match=filter):
        InlineScope([CustomInline(), CustomBlock(), CustomInline()])


def test_inline_scope_must_only_append_inlines():
    filter = r"instance of Inline, but it didn't have property is_inline=True"
    scope = InlineScope([])
    with pytest.raises(TypeError, match=filter):
        scope.append_inline(None)
    with pytest.raises(TypeError, match=filter):
        scope.append_inline(1)
    with pytest.raises(TypeError, match=filter):
        scope.append_inline("blah")
    with pytest.raises(TypeError, match=filter):
        scope.append_inline(CustomBlock())


# Sentence
def test_sentence_can_hold_inlines():
    Sentence([CustomInline(), CustomInline(), CustomInline()])


def test_sentence_can_append_inlines():
    scope = Sentence([])
    scope.append_inline(CustomInline())
    scope.append_inline(Text(""))
    scope.append_inline(Raw(""))
    scope.append_inline(InlineScope([]))


def test_sentence_must_only_have_inlines():
    filter = r"instance of Inline, but it didn't have property is_inline=True"

    with pytest.raises(TypeError, match=filter):
        Sentence([CustomInline(), None, CustomInline()])
    with pytest.raises(TypeError, match=filter):
        Sentence([CustomInline(), 1, CustomInline()])
    with pytest.raises(TypeError, match=filter):
        Sentence([CustomInline(), "blah", CustomInline()])
    with pytest.raises(TypeError, match=filter):
        Sentence([CustomInline(), CustomBlock(), CustomInline()])


def test_sentence_must_only_append_inlines():
    filter = r"instance of Inline, but it didn't have property is_inline=True"
    scope = Sentence([])
    with pytest.raises(TypeError, match=filter):
        scope.append_inline(None)
    with pytest.raises(TypeError, match=filter):
        scope.append_inline(1)
    with pytest.raises(TypeError, match=filter):
        scope.append_inline("blah")
    with pytest.raises(TypeError, match=filter):
        scope.append_inline(CustomBlock())
