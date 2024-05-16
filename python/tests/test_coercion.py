import pytest
from turnip_text import *


class CustomInline:
    is_inline: bool = True


# Inline coercion leaves inline instances as-is, coerces lists of inlines to Inlines, and coerces str, float, and int to Text(str(x))
# Inline scope coercion leaves Inlines instances as-is, coerces lists of inlines to Inlines, coerces standalone Inline instances to Inlines([inline]), and coerces str, float, and int to Inlines([Text(str(x))])


def test_inline_coercion_of_inline():
    t = CustomInline()
    assert coerce_to_inline(t) is t
    assert coerce_to_inlines(t) == Inlines([t])


def test_inline_coercion_of_inline_scope():
    ts = Inlines([CustomInline(), CustomInline(), CustomInline()])
    assert coerce_to_inline(ts) is ts
    assert coerce_to_inlines(ts) is ts


def test_inline_coercion_of_list_of_inline():
    ts = [CustomInline(), CustomInline(), CustomInline()]
    assert coerce_to_inline(ts) == Inlines(ts)
    assert coerce_to_inlines(ts) == Inlines(ts)


def test_inline_coercion_of_tuple_of_inline():
    ts = (CustomInline(), CustomInline(), CustomInline())
    assert coerce_to_inline(ts) == Inlines(ts)
    assert coerce_to_inlines(ts) == Inlines(ts)


def test_inline_coercion_of_str():
    s = "Some Text"
    assert coerce_to_inline(s) == Text(s)
    assert coerce_to_inlines(s) == Inlines([Text(s)])


def test_inline_coercion_of_float():
    f = 1.0
    assert coerce_to_inline(f) == Text(f"{f}")
    assert coerce_to_inlines(f) == Inlines([Text(f"{f}")])


def test_inline_coercion_of_int():
    # Use an integer too large for rust to use natively
    i = 737481293873132131293972839821398293
    assert coerce_to_inline(i) == Text(f"{i}")
    assert coerce_to_inlines(i) == Inlines([Text(f"{i}")])


def test_inline_coercion_of_non_coercible():
    with pytest.raises(TypeError):
        coerce_to_inline(object())
    with pytest.raises(TypeError):
        coerce_to_inlines(object())


class CustomBlock:
    is_block: bool = True


# Block coercion returns Block instances as-is, coerces lists of blocks to Blocks, coerces a Sentence to a Paragraph (but not lists of sentences), and coerces anything coercible to inline into Paragraph([Sentence([coerced_to_inline])])
# Block scope coercion returns Blocks instances as-is, coerces lists of blocks to Blocks, coerces individual Block instances to Blocks([block]), coerces a Sentence to Blocks([Paragraph([sentence])]), and coerces anything coercible to Inline to Blocks([Paragraph([Sentence([coerced_to_inline])])])


def test_block_coercion_of_block():
    b = CustomBlock()
    assert coerce_to_block(b) is b
    assert coerce_to_blocks(b) == Blocks([b])


def test_block_coercion_of_block_scope():
    bs = Blocks([CustomBlock(), CustomBlock(), CustomBlock()])
    assert coerce_to_block(bs) is bs
    assert coerce_to_blocks(bs) is bs


def test_block_coercion_of_list_of_blocks():
    bs = [CustomBlock(), CustomBlock(), CustomBlock()]
    assert coerce_to_block(bs) == Blocks(bs)
    assert coerce_to_blocks(bs) == Blocks(bs)


def test_block_coercion_of_tuple_of_blocks():
    bs = (CustomBlock(), CustomBlock(), CustomBlock())
    assert coerce_to_block(bs) == Blocks(bs)
    assert coerce_to_blocks(bs) == Blocks(bs)


def test_block_coercion_of_sentence():
    s = Sentence([Text("Some Text"), Raw("And Some Raw"), CustomInline()])
    assert coerce_to_block(s) == Paragraph([s])
    assert coerce_to_blocks(s) == Blocks([Paragraph([s])])


def test_block_coercion_of_inline():
    i = CustomInline()
    assert coerce_to_block(i) == Paragraph([Sentence([i])])
    assert coerce_to_blocks(i) == Blocks([Paragraph([Sentence([i])])])


def test_block_coercion_of_inline_scope():
    i = Inlines([CustomInline()])
    assert coerce_to_block(i) == Paragraph([Sentence([i])])
    assert coerce_to_blocks(i) == Blocks([Paragraph([Sentence([i])])])


# List of inlines -> Inlines -> Paragraph(Sentence(Inlines))
def test_block_coercion_of_list_of_inlines():
    i = [CustomInline()]
    assert coerce_to_block(i) == Paragraph([Sentence([Inlines(i)])])
    assert coerce_to_blocks(i) == Blocks([Paragraph([Sentence([Inlines(i)])])])


def test_block_coercion_of_tuple_of_inlines():
    i = (CustomInline(),)
    assert coerce_to_block(i) == Paragraph([Sentence([Inlines(i)])])
    assert coerce_to_blocks(i) == Blocks([Paragraph([Sentence([Inlines(i)])])])


def test_block_coercion_of_str():
    s = "Some Text"
    assert coerce_to_block(s) == Paragraph([Sentence([Text(s)])])
    assert coerce_to_blocks(s) == Blocks([Paragraph([Sentence([Text(s)])])])


def test_block_coercion_of_float():
    f = 1.0
    assert coerce_to_block(f) == Paragraph([Sentence([Text(f"{f}")])])
    assert coerce_to_blocks(f) == Blocks([Paragraph([Sentence([Text(f"{f}")])])])


def test_block_coercion_of_int():
    # Use an integer too large for rust to use natively
    i = 737481293873132131293972839821398293
    assert coerce_to_block(i) == Paragraph([Sentence([Text(f"{i}")])])
    assert coerce_to_blocks(i) == Blocks([Paragraph([Sentence([Text(f"{i}")])])])
