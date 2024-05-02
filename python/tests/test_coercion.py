import pytest
from turnip_text import *


class CustomInline:
    is_inline: bool = True

    def __eq__(self, value: object) -> bool:
        return isinstance(value, CustomInline)


# Inline coercion leaves inline instances as-is, coerces lists of inlines to InlineScope, and coerces str, float, and int to Text(str(x))
# Inline scope coercion leaves InlineScope instances as-is, coerces lists of inlines to InlineScope, coerces standalone Inline instances to InlineScope([inline]), and coerces str, float, and int to InlineScope([Text(str(x))])


def test_inline_coercion_of_inline():
    t = CustomInline()
    assert coerce_to_inline(t) is t
    assert coerce_to_inline_scope(t) == InlineScope([t])


def test_inline_coercion_of_inline_scope():
    ts = InlineScope([CustomInline(), CustomInline(), CustomInline()])
    assert coerce_to_inline(ts) is ts
    assert coerce_to_inline_scope(ts) is ts


def test_inline_coercion_of_list_of_inline():
    ts = [CustomInline(), CustomInline(), CustomInline()]
    assert coerce_to_inline(ts) == InlineScope(ts)
    assert coerce_to_inline_scope(ts) == InlineScope(ts)


def test_inline_coercion_of_str():
    s = "Some Text"
    assert coerce_to_inline(s) == Text(s)
    assert coerce_to_inline_scope(s) == InlineScope([Text(s)])


def test_inline_coercion_of_float():
    f = 1.0
    assert coerce_to_inline(f) == Text(f"{f}")
    assert coerce_to_inline_scope(f) == InlineScope([Text(f"{f}")])


def test_inline_coercion_of_int():
    # Use an integer too large for rust to use directly
    i = 737481293873132131293972839821398293
    assert coerce_to_inline(i) == Text(f"{i}")
    assert coerce_to_inline_scope(i) == InlineScope([Text(f"{i}")])


def test_inline_coercion_of_non_coercible():
    with pytest.raises(TypeError):
        coerce_to_inline(object())
    with pytest.raises(TypeError):
        coerce_to_inline_scope(object())


class CustomBlock:
    is_block: bool = True


# Block coercion returns Block instances as-is, coerces lists of blocks to BlockScope, coerces a Sentence to a Paragraph (but not lists of sentences), and coerces anything coercible to inline into Paragraph([Sentence([coerced_to_inline])])
# Block scope coercion returns BlockScope instances as-is, coerces lists of blocks to BlockScope, coerces individual Block instances to BlockScope([block]), coerces a Sentence to BlockScope([Paragraph([sentence])]), and coerces anything coercible to Inline to BlockScope([Paragraph([Sentence([coerced_to_inline])])])


def test_block_coercion_of_block():
    b = CustomBlock()
    assert coerce_to_block(b) is b
    assert coerce_to_block_scope(b) == BlockScope([b])


def test_block_coercion_of_block_scope():
    bs = BlockScope([CustomBlock(), CustomBlock(), CustomBlock()])
    assert coerce_to_block(bs) is bs
    assert coerce_to_block_scope(bs) is bs


def test_block_coercion_of_list_of_blocks():
    bs = [CustomBlock(), CustomBlock(), CustomBlock()]
    assert coerce_to_block(bs) == BlockScope(bs)
    assert coerce_to_block_scope(bs) == BlockScope(bs)


def test_block_coercion_of_sentence():
    s = Sentence([Text("Some Text"), Raw("And Some Raw"), CustomInline()])
    assert coerce_to_block(s) == Paragraph([s])
    assert coerce_to_block_scope(s) == BlockScope([Paragraph([s])])


def test_block_coercion_of_inline():
    i = CustomInline()
    assert coerce_to_block(i) == Paragraph([Sentence([i])])
    assert coerce_to_block_scope(i) == BlockScope([Paragraph([Sentence([i])])])


def test_block_coercion_of_inline_scope():
    i = InlineScope([CustomInline()])
    assert coerce_to_block(i) == Paragraph([Sentence([i])])
    assert coerce_to_block_scope(i) == BlockScope([Paragraph([Sentence([i])])])


# List of inlines -> InlineScope -> Paragraph(Sentence(InlineScope))
def test_block_coercion_of_list_of_inlines():
    i = [CustomInline()]
    assert coerce_to_block(i) == Paragraph([Sentence([InlineScope(i)])])
    assert coerce_to_block_scope(i) == BlockScope(
        [Paragraph([Sentence([InlineScope(i)])])]
    )


def test_block_coercion_of_str():
    s = "Some Text"
    assert coerce_to_block(s) == Paragraph([Sentence([Text(s)])])
    assert coerce_to_block_scope(s) == BlockScope([Paragraph([Sentence([Text(s)])])])


def test_block_coercion_of_float():
    f = 1.0
    assert coerce_to_block(f) == Paragraph([Sentence([Text(f"{f}")])])
    assert coerce_to_block_scope(f) == BlockScope(
        [Paragraph([Sentence([Text(f"{f}")])])]
    )


def test_block_coercion_of_int():
    # Use an integer too large for rust to use directly
    i = 737481293873132131293972839821398293
    assert coerce_to_block(i) == Paragraph([Sentence([Text(f"{i}")])])
    assert coerce_to_block_scope(i) == BlockScope(
        [Paragraph([Sentence([Text(f"{i}")])])]
    )
