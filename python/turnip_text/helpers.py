from typing import Any, Callable, List, Optional, Union

from turnip_text import (
    Block,
    BlockScope,
    BlockScopeBuilder,
    CoercibleToBlockScope,
    CoercibleToInlineScope,
    DocSegmentHeader,
    Inline,
    InlineScope,
    InlineScopeBuilder,
    RawScopeBuilder,
    coerce_to_block_scope,
    coerce_to_inline_scope,
)

# TODO tests for the helpers


class block_scope_builder(BlockScopeBuilder):
    """
    Decorator which allows a function to fit the BlockScopeBuilder typeclass.

    e.g. one could define a function
    ```python
    def block(name=""):
        @block_scope_builder
        def inner(items: BlockScope) -> Block:
            return items
        return inner
    ```
    which allows turnip-text as so:
    ```!text
    [block(name="greg")]{
    The contents of greg
    }
    ```

    It also supports the matmul operator, which tries to coerce the right-hand-side into a BlockScope before calling the function:
    ```python
    block(name="greg") @ "The contents of greg"
    ```
    """

    func: Callable[[BlockScope], Optional[Block | DocSegmentHeader]]

    def __init__(self, func: Callable[[BlockScope], Optional[Block | DocSegmentHeader]]) -> None:
        self.func = func

    def build_from_blocks(self, b: BlockScope) -> Optional[Block | DocSegmentHeader]:
        return self.func(b)


class inline_scope_builder(InlineScopeBuilder):
    """
    Decorator which allows a function to fit the InlineScopeBuilder typeclass.

    e.g. one could define a function
    ```python
    def inline(postfix = ""):
        @inline_scope_builder
        def inner(items: InlineScope) -> Inline:
            return InlineScope(list(items) + [postfix])
        return inner
    ```
    which allows turnip-text as so:
    ```!text
    [inline("!")]{surprise}
    ```

    It also supports the matmul operator, which tries to coerce the right-hand-side into an InlineScope before calling the function:
    ```python
    inline("!") @ "surprise"
    ```
    """

    func: Callable[[InlineScope], Inline]

    def __init__(self, func: Callable[[InlineScope], Inline]) -> None:
        self.func = func

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        return self.func(inls)


class raw_scope_builder(RawScopeBuilder):
    """
    Decorator which allows a function to fit the RawScopeBuilder typeclass.

    e.g. one could define a function
    ```python
    @raw_scope_builder
    def math(raw_text: str) -> Inline:
        ...
    ```
    which allows turnip-text as so:
    ```!text
    [math]#{\\sin x}#
    ```

    It also supports the matmul operator, which checks the right-hand-side is a str before calling the function:
    ```python
    math @ r"\\sin x"
    ```
    """

    func: Callable[[str], Union[Block, Inline]]

    def __init__(self, func: Callable[[str], Union[Block, Inline]]) -> None:
        self.func = func

    def build_from_raw(self, raw: str) -> Union[Block, Inline]:
        return self.func(raw)

    def __matmul__(self, maybe_str: Any) -> Union[Block, Inline]:
        if isinstance(maybe_str, str):
            return self.func(maybe_str)
        raise TypeError(
            f"Invoked RawScopeBuilder on {maybe_str}, which wasn't a string"
        )
