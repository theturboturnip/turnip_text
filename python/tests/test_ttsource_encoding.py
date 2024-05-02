import io

from turnip_text import *


def test_utf8():
    original_src = r"""
ði ıntəˈnæʃənəl fəˈnɛtık əsoʊsiˈeıʃn
Y \[ˈʏpsilɔn\], Yen \[jɛn\], Yoga \[ˈjoːgɑ\]
"""
    utf8 = io.StringIO(
        initial_value=str(original_src.encode(encoding="utf-8"), encoding="utf-8")
    )
    src = TurnipTextSource.from_file("<test>", utf8)
    doc = parse_file_native(src, {})
    expected_doc = Document(
        contents=BlockScope(
            [
                Paragraph(
                    [
                        Sentence([Text("ði ıntəˈnæʃənəl fəˈnɛtık əsoʊsiˈeıʃn")]),
                        Sentence([Text("Y [ˈʏpsilɔn], Yen [jɛn], Yoga [ˈjoːgɑ]")]),
                    ]
                )
            ]
        ),
        segments=[],
    )
    print(expected_doc)
    print(doc)
    assert expected_doc == doc
