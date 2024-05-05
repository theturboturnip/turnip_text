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


# TODO test error messages
