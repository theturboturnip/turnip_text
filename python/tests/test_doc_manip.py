from dataclasses import dataclass

import pytest

from turnip_text import *


@dataclass  # for equality testing
class CustomHeader:
    is_header: bool = True
    weight: int = 0

    def __init__(self, weight: int = 0) -> None:
        self.weight = weight


def test_doc_appends_headers_correctly():
    # When appending headers, if each subsequent weight is <= the previous weight, you get multiple segments at the top level
    doc = Document(contents=BlockScope(), segments=[])
    doc.append_header(CustomHeader(weight=10))
    doc.append_header(CustomHeader(weight=10))
    doc.append_header(CustomHeader(weight=10))
    doc.append_header(CustomHeader(weight=10))
    print(doc)
    assert doc == Document(
        contents=BlockScope(),
        segments=[DocSegment(CustomHeader(weight=10), BlockScope(), [])] * 4,
    )

    doc = Document(contents=BlockScope(), segments=[])
    doc.append_header(CustomHeader(weight=10))
    doc.append_header(CustomHeader(weight=9))
    doc.append_header(CustomHeader(weight=8))
    doc.append_header(CustomHeader(weight=7))
    assert doc == Document(
        contents=BlockScope(),
        segments=[
            DocSegment(CustomHeader(weight=10), BlockScope(), []),
            DocSegment(CustomHeader(weight=9), BlockScope(), []),
            DocSegment(CustomHeader(weight=8), BlockScope(), []),
            DocSegment(CustomHeader(weight=7), BlockScope(), []),
        ],
    )

    # When appending headers when weight > previous, it nests
    doc = Document(contents=BlockScope(), segments=[])
    doc.append_header(CustomHeader(weight=1))
    doc.append_header(CustomHeader(weight=2))
    doc.append_header(CustomHeader(weight=3))
    doc.append_header(CustomHeader(weight=4))
    assert doc == Document(
        contents=BlockScope(),
        segments=[
            DocSegment(
                CustomHeader(weight=1),
                BlockScope(),
                [
                    DocSegment(
                        CustomHeader(weight=2),
                        BlockScope(),
                        [
                            DocSegment(
                                CustomHeader(weight=3),
                                BlockScope(),
                                [
                                    DocSegment(
                                        CustomHeader(weight=4), BlockScope(), []
                                    ),
                                ],
                            ),
                        ],
                    ),
                ],
            ),
        ],
    )

    doc = Document(contents=BlockScope(), segments=[])
    doc.append_header(CustomHeader(weight=1))
    doc.append_header(CustomHeader(weight=4))
    doc.append_header(CustomHeader(weight=1))
    doc.append_header(CustomHeader(weight=2))
    assert doc == Document(
        contents=BlockScope(),
        segments=[
            DocSegment(
                CustomHeader(weight=1),
                BlockScope(),
                [
                    DocSegment(
                        CustomHeader(weight=4),
                        BlockScope(),
                        [],
                    ),
                ],
            ),
            DocSegment(
                CustomHeader(weight=1),
                BlockScope(),
                [
                    DocSegment(
                        CustomHeader(weight=2),
                        BlockScope(),
                        [],
                    ),
                ],
            ),
        ],
    )


def test_doc_inserts_headers_correctly():
    # Port the examples from the documentation
    ## The new docsegment may be pre-created with children if its weight is smaller than subsequent elements.
    ## For example if the list has three elements `[A, B, C]` and `X` is inserted after `A`, there are four possiblities:
    ## - `[A.append(X), B, C]` is allowed if `A.weight < X.weight`
    ##     - e.g. A = 100, X = 110, B = 75, C = 50
    ## - `[A, X, B, C]` is allowed if `A.weight >= X.weight` and `X.weight >= B.weight`
    ##     - e.g. A = 100, X = 90, B = 75, C = 50
    ## - `[A, X.append(B), C]` is allowed if `A.weight >= X.weight`, `X.weight < B.weight`, and `X.weight >= C.weight`
    ##     - e.g. A = 100, X = 60, B = 75, C = 50
    ## - `[A, X.append(B, C)]` is allowed if `A.weight >= X.weight`, `X.weight < B.weight`, and `X.weight < C.weight`
    ##     - e.g. A = 100, X = 10, B = 75, C = 50

    def starting_doc():
        doc = Document(BlockScope(), [])
        doc.append_header(CustomHeader(100))
        doc.append_header(CustomHeader(75))
        doc.append_header(CustomHeader(50))
        return doc

    # Insert X = 110
    ## - `[A.append(X), B, C]` is allowed if `A.weight < X.weight`
    doc = starting_doc()
    inserted = doc.insert_header(1, CustomHeader(110))
    # X should have no children
    assert inserted == DocSegment(CustomHeader(110), BlockScope(), [])
    assert doc == Document(
        BlockScope(),
        [
            # A
            DocSegment(
                CustomHeader(100),
                BlockScope(),
                [
                    # X
                    DocSegment(CustomHeader(110), BlockScope(), []),
                ],
            ),
            # B
            DocSegment(CustomHeader(75), BlockScope(), []),
            # C
            DocSegment(CustomHeader(50), BlockScope(), []),
        ],
    )

    # Inserted X = 90
    ## - `[A, X, B, C]` is allowed if `A.weight >= X.weight` and `X.weight >= B.weight`
    doc = starting_doc()
    inserted = doc.insert_header(1, CustomHeader(90))
    # X should have no children
    assert inserted == DocSegment(CustomHeader(90), BlockScope(), [])
    assert doc == Document(
        BlockScope(),
        [
            # A
            DocSegment(CustomHeader(100), BlockScope(), []),
            # X
            DocSegment(CustomHeader(90), BlockScope(), []),
            # B
            DocSegment(CustomHeader(75), BlockScope(), []),
            # C
            DocSegment(CustomHeader(50), BlockScope(), []),
        ],
    )

    # Inserted X = 60
    ## - `[A, X.append(B), C]` is allowed if `A.weight >= X.weight`, `X.weight < B.weight`, and `X.weight >= C.weight`
    doc = starting_doc()
    inserted = doc.insert_header(1, CustomHeader(60))
    # X should have one child
    assert inserted == DocSegment(
        CustomHeader(60),
        BlockScope(),
        [
            # B
            DocSegment(CustomHeader(75), BlockScope(), []),
        ],
    )
    assert doc == Document(
        BlockScope(),
        [
            # A
            DocSegment(CustomHeader(100), BlockScope(), []),
            # X
            DocSegment(
                CustomHeader(60),
                BlockScope(),
                [
                    # B
                    DocSegment(CustomHeader(75), BlockScope(), []),
                ],
            ),
            # C
            DocSegment(CustomHeader(50), BlockScope(), []),
        ],
    )

    # Inserted X = 10
    ## - `[A, X.append(B, C)]` is allowed if `A.weight >= X.weight`, `X.weight < B.weight`, and `X.weight < C.weight`
    doc = starting_doc()
    inserted = doc.insert_header(1, CustomHeader(10))
    # X should have two children
    assert inserted == DocSegment(
        CustomHeader(10),
        BlockScope(),
        [
            # B
            DocSegment(CustomHeader(75), BlockScope(), []),
            # C
            DocSegment(CustomHeader(50), BlockScope(), []),
        ],
    )
    assert doc == Document(
        BlockScope(),
        [
            # A
            DocSegment(CustomHeader(100), BlockScope(), []),
            # X
            DocSegment(
                CustomHeader(10),
                BlockScope(),
                [
                    # B
                    DocSegment(CustomHeader(75), BlockScope(), []),
                    # C
                    DocSegment(CustomHeader(50), BlockScope(), []),
                ],
            ),
        ],
    )


def test_doc_segment_prevents_smaller_weights():
    # DocSegment doesn't allow insertion of subsegments with lower or equal weight to the docsegment itself
    doc_seg = DocSegment(CustomHeader(weight=10), BlockScope(), [])

    with pytest.raises(ValueError):
        doc_seg.append_header(CustomHeader(weight=5))
    with pytest.raises(ValueError):
        doc_seg.append_header(CustomHeader(weight=9))
    with pytest.raises(ValueError):
        doc_seg.append_header(CustomHeader(weight=10))
    doc_seg.append_header(CustomHeader(weight=11))
    doc_seg.append_header(CustomHeader(weight=15))


def test_doc_segment_appends_headers_correctly():
    # When appending headers, if each subsequent weight is <= the previous weight, you get multiple segments at the top level
    doc = DocSegment(CustomHeader(weight=0), contents=BlockScope(), subsegments=[])
    doc.append_header(CustomHeader(weight=10))
    doc.append_header(CustomHeader(weight=10))
    doc.append_header(CustomHeader(weight=10))
    doc.append_header(CustomHeader(weight=10))
    print(doc)
    assert doc == DocSegment(
        CustomHeader(weight=0),
        contents=BlockScope(),
        subsegments=[DocSegment(CustomHeader(weight=10), BlockScope(), [])] * 4,
    )

    doc = DocSegment(CustomHeader(weight=0), contents=BlockScope(), subsegments=[])
    doc.append_header(CustomHeader(weight=10))
    doc.append_header(CustomHeader(weight=9))
    doc.append_header(CustomHeader(weight=8))
    doc.append_header(CustomHeader(weight=7))
    assert doc == DocSegment(
        CustomHeader(weight=0),
        contents=BlockScope(),
        subsegments=[
            DocSegment(CustomHeader(weight=10), BlockScope(), []),
            DocSegment(CustomHeader(weight=9), BlockScope(), []),
            DocSegment(CustomHeader(weight=8), BlockScope(), []),
            DocSegment(CustomHeader(weight=7), BlockScope(), []),
        ],
    )

    # When appending headers when weight > previous, it nests
    doc = DocSegment(CustomHeader(weight=0), contents=BlockScope(), subsegments=[])
    doc.append_header(CustomHeader(weight=1))
    doc.append_header(CustomHeader(weight=2))
    doc.append_header(CustomHeader(weight=3))
    doc.append_header(CustomHeader(weight=4))
    assert doc == DocSegment(
        CustomHeader(weight=0),
        contents=BlockScope(),
        subsegments=[
            DocSegment(
                CustomHeader(weight=1),
                BlockScope(),
                [
                    DocSegment(
                        CustomHeader(weight=2),
                        BlockScope(),
                        [
                            DocSegment(
                                CustomHeader(weight=3),
                                BlockScope(),
                                [
                                    DocSegment(
                                        CustomHeader(weight=4), BlockScope(), []
                                    ),
                                ],
                            ),
                        ],
                    ),
                ],
            ),
        ],
    )

    doc = DocSegment(CustomHeader(weight=0), contents=BlockScope(), subsegments=[])
    doc.append_header(CustomHeader(weight=1))
    doc.append_header(CustomHeader(weight=4))
    doc.append_header(CustomHeader(weight=1))
    doc.append_header(CustomHeader(weight=2))
    assert doc == DocSegment(
        CustomHeader(weight=0),
        contents=BlockScope(),
        subsegments=[
            DocSegment(
                CustomHeader(weight=1),
                BlockScope(),
                [
                    DocSegment(
                        CustomHeader(weight=4),
                        BlockScope(),
                        [],
                    ),
                ],
            ),
            DocSegment(
                CustomHeader(weight=1),
                BlockScope(),
                [
                    DocSegment(
                        CustomHeader(weight=2),
                        BlockScope(),
                        [],
                    ),
                ],
            ),
        ],
    )


def test_doc_segment_inserts_headers_correctly():
    # Port the examples from the documentation
    ## The new docsegment may be pre-created with children if its weight is smaller than subsequent elements.
    ## For example if the list has three elements `[A, B, C]` and `X` is inserted after `A`, there are four possiblities:
    ## - `[A.append(X), B, C]` is allowed if `A.weight < X.weight`
    ##     - e.g. A = 100, X = 110, B = 75, C = 50
    ## - `[A, X, B, C]` is allowed if `A.weight >= X.weight` and `X.weight >= B.weight`
    ##     - e.g. A = 100, X = 90, B = 75, C = 50
    ## - `[A, X.append(B), C]` is allowed if `A.weight >= X.weight`, `X.weight < B.weight`, and `X.weight >= C.weight`
    ##     - e.g. A = 100, X = 60, B = 75, C = 50
    ## - `[A, X.append(B, C)]` is allowed if `A.weight >= X.weight`, `X.weight < B.weight`, and `X.weight < C.weight`
    ##     - e.g. A = 100, X = 10, B = 75, C = 50

    def starting_doc():
        doc = DocSegment(CustomHeader(0), BlockScope(), [])
        doc.append_header(CustomHeader(100))
        doc.append_header(CustomHeader(75))
        doc.append_header(CustomHeader(50))
        return doc

    # Insert X = 110
    ## - `[A.append(X), B, C]` is allowed if `A.weight < X.weight`
    doc = starting_doc()
    inserted = doc.insert_header(1, CustomHeader(110))
    # X should have no children
    assert inserted == DocSegment(CustomHeader(110), BlockScope(), [])
    assert doc == DocSegment(
        CustomHeader(0),
        BlockScope(),
        [
            # A
            DocSegment(
                CustomHeader(100),
                BlockScope(),
                [
                    # X
                    DocSegment(CustomHeader(110), BlockScope(), []),
                ],
            ),
            # B
            DocSegment(CustomHeader(75), BlockScope(), []),
            # C
            DocSegment(CustomHeader(50), BlockScope(), []),
        ],
    )

    # Inserted X = 90
    ## - `[A, X, B, C]` is allowed if `A.weight >= X.weight` and `X.weight >= B.weight`
    doc = starting_doc()
    inserted = doc.insert_header(1, CustomHeader(90))
    # X should have no children
    assert inserted == DocSegment(CustomHeader(90), BlockScope(), [])
    assert doc == DocSegment(
        CustomHeader(0),
        BlockScope(),
        [
            # A
            DocSegment(CustomHeader(100), BlockScope(), []),
            # X
            DocSegment(CustomHeader(90), BlockScope(), []),
            # B
            DocSegment(CustomHeader(75), BlockScope(), []),
            # C
            DocSegment(CustomHeader(50), BlockScope(), []),
        ],
    )

    # Inserted X = 60
    ## - `[A, X.append(B), C]` is allowed if `A.weight >= X.weight`, `X.weight < B.weight`, and `X.weight >= C.weight`
    doc = starting_doc()
    inserted = doc.insert_header(1, CustomHeader(60))
    # X should have one child
    assert inserted == DocSegment(
        CustomHeader(60),
        BlockScope(),
        [
            # B
            DocSegment(CustomHeader(75), BlockScope(), []),
        ],
    )
    assert doc == DocSegment(
        CustomHeader(0),
        BlockScope(),
        [
            # A
            DocSegment(CustomHeader(100), BlockScope(), []),
            # X
            DocSegment(
                CustomHeader(60),
                BlockScope(),
                [
                    # B
                    DocSegment(CustomHeader(75), BlockScope(), []),
                ],
            ),
            # C
            DocSegment(CustomHeader(50), BlockScope(), []),
        ],
    )

    # Inserted X = 10
    ## - `[A, X.append(B, C)]` is allowed if `A.weight >= X.weight`, `X.weight < B.weight`, and `X.weight < C.weight`
    doc = starting_doc()
    inserted = doc.insert_header(1, CustomHeader(10))
    # X should have two children
    assert inserted == DocSegment(
        CustomHeader(10),
        BlockScope(),
        [
            # B
            DocSegment(CustomHeader(75), BlockScope(), []),
            # C
            DocSegment(CustomHeader(50), BlockScope(), []),
        ],
    )
    assert doc == DocSegment(
        CustomHeader(0),
        BlockScope(),
        [
            # A
            DocSegment(CustomHeader(100), BlockScope(), []),
            # X
            DocSegment(
                CustomHeader(10),
                BlockScope(),
                [
                    # B
                    DocSegment(CustomHeader(75), BlockScope(), []),
                    # C
                    DocSegment(CustomHeader(50), BlockScope(), []),
                ],
            ),
        ],
    )
