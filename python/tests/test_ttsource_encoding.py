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
    assert expected_doc == doc


# Even after putting the text through UTF-16, turnip-text should still handle it
def test_utf16():
    original_src = r"""
ði ıntəˈnæʃənəl fəˈnɛtık əsoʊsiˈeıʃn
Y \[ˈʏpsilɔn\], Yen \[jɛn\], Yoga \[ˈjoːgɑ\]
"""
    utf8 = io.StringIO(
        initial_value=str(original_src.encode(encoding="utf-16be"), encoding="utf-16be")
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
    assert expected_doc == doc


# Even after putting the text through SHIFT-JIS, turnip-text should still handle it
def test_shift_jis():
    original_src = r"""
A｡｢｣､･ｦｧｨｩｪｫｬｭｮｯ
Bｰｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿ
Cﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏ
Dﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝﾞﾟ
"""
    utf8 = io.StringIO(
        initial_value=str(
            original_src.encode(encoding="shift-jis"), encoding="shift-jis"
        )
    )
    src = TurnipTextSource.from_file("<test>", utf8)
    doc = parse_file_native(src, {})
    # Even comparing the doc to one generated from UTF-8 should work
    expected_doc = Document(
        contents=BlockScope(
            [
                Paragraph(
                    [
                        Sentence([Text("A｡｢｣､･ｦｧｨｩｪｫｬｭｮｯ")]),
                        Sentence([Text("Bｰｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿ")]),
                        Sentence([Text("Cﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏ")]),
                        Sentence([Text("Dﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝﾞﾟ")]),
                    ]
                )
            ]
        ),
        segments=[],
    )
    assert expected_doc == doc
