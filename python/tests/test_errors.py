import io
import os
import re
import traceback
from pathlib import Path

import pytest

from turnip_text import *

SPECIFIC_ERROR = RuntimeError("An Error")


def raise_specific_error():
    raise SPECIFIC_ERROR


def nest1_raise_specific():
    raise_specific_error()


def nest2_raise_specific():
    nest1_raise_specific()


def nest3_raise_specific():
    nest2_raise_specific()


def test_selfcheck():
    with pytest.raises(RuntimeError) as err_info:
        nest3_raise_specific()
    assert err_info.value is SPECIFIC_ERROR


def test_error_from_running_user_code_filters_out():
    with pytest.raises(TurnipTextError) as err_info:
        parse_file(
            TurnipTextSource.from_string("[raise ValueError()]"),
            {},
        )
    assert isinstance(err_info.value.__cause__, ValueError)
    assert isinstance(err_info.value.__context__, ValueError)


def test_error_from_env_running_user_code_filters_out():
    with pytest.raises(TurnipTextError) as err_info:
        parse_file(
            TurnipTextSource.from_string("[raise_specific_error()]"),
            {"raise_specific_error": raise_specific_error},
        )
    assert err_info.value.__cause__ is SPECIFIC_ERROR
    assert err_info.value.__context__ is SPECIFIC_ERROR


def test_error_from_nested_env_running_user_code_filters_out():
    with pytest.raises(TurnipTextError) as err_info:
        parse_file(
            TurnipTextSource.from_string("[nest3_raise_specific()]"),
            {"nest3_raise_specific": nest3_raise_specific},
        )
    assert err_info.value.__cause__ is SPECIFIC_ERROR
    assert err_info.value.__context__ is SPECIFIC_ERROR


def test_error_from_nested_file_running_user_code_filters_out():
    with pytest.raises(TurnipTextError) as err_info:
        parse_file(
            TurnipTextSource.from_string(
                """
                [-import turnip_text as tt-]
                [-
                tt.TurnipTextSource.from_string("[nest3_raise_specific()]")
                -]"""
            ),
            {"nest3_raise_specific": nest3_raise_specific},
        )
    assert err_info.value.__cause__ is SPECIFIC_ERROR
    assert err_info.value.__context__ is SPECIFIC_ERROR


def test_print_error_messages():
    with open(
        os.path.join(os.path.dirname(__file__), "error_messages.txt"),
        mode="w",
        encoding="utf-8",
    ) as f:

        def test_one_error(reason: str, data: str, py_env=None):
            with pytest.raises(TurnipTextError) as err_info:
                parse_file(
                    TurnipTextSource.from_string(data),
                    py_env if py_env else {},
                    recursion_warning=False,
                    max_file_depth=16,
                )
            traceback_msg_buf = io.StringIO()
            traceback.print_exception(err_info.value, file=traceback_msg_buf)
            # Replace filepaths with base names
            traceback_msg = re.sub(
                r"\"([\w:\\/\s\.]+)\"",
                lambda match: Path(match.group(1)).name,
                traceback_msg_buf.getvalue(),
            )
            # Replace <object Blah at 0x0928023> with <object Blah at MEMADDR>
            # These replacements mean the traceback remains stable across computers
            traceback_msg = re.sub(r"at 0x[\da-fA-F]+>", "at 0xMEMADDR>", traceback_msg)
            header = "#" * len(reason)
            safe_data = data.replace("\0", "\\0")
            f.write(
                f"\n\n{header}\n{reason}\n{header}\n{safe_data}\n{header}\n{traceback_msg}\n{header}"
            )

        test_one_error("Null Byte in Source", "oidnowbiadw\0unbwodubqdop")
        test_one_error(
            "File Stack Exceeded Limit",
            """[-
            import turnip_text as tt
            s = tt.TurnipTextSource("<test>", "[s]")
            -]
            
            [s]""",
        )
        test_one_error(
            "Syntax - CodeCloseOutsideCode",
            "Wow some stuff in a paragraph\nand then a bare ]\n",
        )
        test_one_error(
            "Syntax - BlockScopeCloseOutsideScope",
            "Wow some stuff in a paragraph\nand then a bare\n}",
        )
        test_one_error(
            "Syntax - InlineScopeCloseOutsideScope",
            "Wow some stuff in a paragraph\nand then a bare }",
        )
        test_one_error(
            "Syntax - RawScopeCloseOutsideScope",
            "Wow some stuff in a paragraph\nand then a bare }###",
        )
        test_one_error(
            "Syntax - EndedInsideCode",
            "Wow some stuff in a paragraph with [ everywhere!\n[[[ ",
        )
        test_one_error(
            "Syntax - EndedInsideRawScope",
            "Wow some stuff in a paragraph with ###{ everywhere!\n ##{ #{",
        )
        test_one_error(
            "Syntax - EndedInsideScope - inline",
            "Wow some stuff in a paragraph with { everywhere! {{{ ",
        )
        test_one_error(
            "Syntax - EndedInsideScope - block",
            "{\nWow some stuff in a paragraph and no closing \\}",
        )

        test_one_error(
            "Syntax - BlockScopeOpenedInInlineMode",
            """
Wahey a very big paragraph.
With multiple sentences, even.
Even some delightful inline scopes { like this one {
    But then SURPRISE a block scope!
}
""",
        )
        test_one_error(
            "Syntax - CodeEmittedBlockInInlineMode",
            """
[-
class CustomBlock:
    is_block=True
-]
And we're inside a paragraph but then [CustomBlock()]!
""",
        )
        test_one_error(
            "Syntax - CodeEmittedHeaderInInlineMode",
            """
[-
class CustomHeader:
    is_header = True
    weight = 12

class CustomHeaderBuilder:
    def build_from_inlines(self, arg):
        return CustomHeader()
-]

And we're inside a paragraph but then { even inside an inline scope [CustomHeaderBuilder()]{with some swallowed inline content} }
""",
        )
        test_one_error(
            "Syntax - CodeEmittedHeaderInBlockScope",
            """
[-
class CustomHeader:
    is_header = True
    weight = 12

class CustomHeaderBuilder:
    def build_from_raw(self, arg):
        return CustomHeader()
-]

{
    inside a block scope

    [CustomHeaderBuilder()]#{and we try to build a header!}#
}
""",
        )
        test_one_error(
            "Syntax - CodeEmittedSourceInInlineMode",
            """
[-
import turnip_text as tt
s = tt.TurnipTextSource.from_string("")

class CustomBuilderFromInline:
    def build_from_inlines(self, arg):
        return None
-]

We're in a paragraph and then
[CustomBuilderFromInline()]{ wow we're inside an inline scope and we emit a source [s] }
""",
        )
        test_one_error(
            "Syntax - SentenceBreakInInlineScope",
            "{ Wow check out this great inline scope\n\n oh, surprise newline }",
        )
        test_one_error(
            "Syntax - EscapedNewlineInBlockMode",
            """{
            
            Inside a block scope there may be a paragraph

            # and then an escaped newline
                \\\n
            }""",
        )
        test_one_error(
            "Syntax - InsufficientBlockSeparation",
            """
Wow we have a big paragraph here
It has so many sentences
And then we could try to make a block right afterwards
{

}
""",
        )
        test_one_error("UserPython - Compiling Statement", "[1.0f]")
        test_one_error(
            "UserPython - Compiling Indented",
            "[    indented = 1\nunindented=1\n    indented=2]",
        )

        def raising_error():
            raise ValueError()

        test_one_error(
            "UserPython - Running",
            "[function_raising_error()]",
            py_env={"function_raising_error": raising_error},
        )
        test_one_error(
            "UserPython - Running Indented",
            "[\n    indented=1\n    function_raising_error()]",
            py_env={"function_raising_error": raising_error},
        )
        test_one_error(
            "UserPython - CoercingEvalBracketToElement - fits none",
            "[object()]",  # fits none of None | Builder | Header | CoercibleToInline
        )
        test_one_error(
            "UserPython - CoercingEvalBracketToElement - fits many",
            """[-
            class FitMany:
                is_block = True
                is_inline = True
                is_header = True
                weight = 0
            -]
            [FitMany()]""",  # fits Builder | Header | Inline
        )
        test_one_error(
            "UserPython - CoercingEvalBracketToBuilder",
            """
            [-
            def returns_builder():
                class Builder:
                    def build_from_blocks(self, arg):
                        return arg
                return Builder()
            -]
            [returns_builder]{ # a function that isn't called
                Wasn't a builder
                But it's expected to receive a block scope
            }""",
        )
        test_one_error(
            "UserPython - CoercingEvalBracketToBuilder - other builder",
            """
[-
class Builder:
    def build_from_raw(self, arg):
        raise ValueError()
-]

[Builder()]{
    block content
}""",
        )
        test_one_error(
            "UserPython - Building",
            """
[-
class Builder:
    def build_from_inlines(self, inls):
        raise ValueError()
-]

[Builder()]{inline content}""",
        )
        test_one_error(
            "UserPython - CoercingBuildResultToElement",
            """
            [-
            class Builder:
                def build_from_blocks(self, arg):
                    return 15 # Not a valid element
            -]
            [Builder()]{
                some valid to build
            }
            """,
        )
        test_one_error(
            "Error message in multibyte text",
            """
A ｡ ｢ ｣ ､ ･ ｦ ｧ ｨ ｩ ｪ ｫ ｬ ｭ ｮ ｯ
B ｰ ｱ ｲ ｳ ｴ ｵ ｶ ｷ ｸ ｹ ｺ ｻ ｼ ｽ ｾ ｿ
C ﾀ ﾁ ﾂ ﾃ ﾄ ﾅ ﾆ ﾇ ﾈ ﾉ ﾊ ﾋ ﾌ ﾍ ﾎ ﾏ
D ﾐ ﾑ ﾒ ﾓ ﾔ ﾕ ﾖ {  ﾗ ﾘ ﾙ ﾚ ﾛ ﾜ ﾝﾞﾟ error unterminated inline scope
""",
        )
