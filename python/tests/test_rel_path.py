import pytest

from turnip_text.build_system import RelPath


def test_basic_rel_path():
    path = RelPath("a/b/c/d", "e/f/g", "/1/2/3/4/5/67/", "document.tex")
    assert str(path) == "a/b/c/d/e/f/g/1/2/3/4/5/67/document.tex"


def test_rel_path_characters():
    # This is a valid component, it should construct successfully
    RelPath("...something.texqwertyuiopASDFGHJKL1234567890-_.tex")


def test_rel_path_dot():
    path = RelPath(".", "1/2/./3", "./4/5/6", ".")
    assert str(path) == "1/2/3/4/5/6"


def test_rel_path_double_dot():
    # Double dots which go too far raise ValueError
    with pytest.raises(ValueError):
        RelPath("..")

    with pytest.raises(ValueError):
        RelPath("1/2/3/4/5/../../../../../../")

    # Double dots which go up to the top level don't
    assert str(RelPath("1/2/3/../../../")) == ""

    # Double dots in the middle of paths do the right thing
    assert str(RelPath("1/2/../2a")) == "1/2a"


def test_rel_path_rejects_triple_dots():
    with pytest.raises(ValueError):
        RelPath("123/...")
    with pytest.raises(ValueError):
        RelPath("123/....")
    with pytest.raises(ValueError):
        RelPath("123/.....")
    with pytest.raises(ValueError):
        RelPath("123/......")
