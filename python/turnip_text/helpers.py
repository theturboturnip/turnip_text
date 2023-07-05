from typing import Any, Callable, List, Optional, Union

from turnip_text import (
    Block,
    BlockScope,
    BlockScopeBuilder,
    Inline,
    InlineScope,
    InlineScopeBuilder,
    RawScopeBuilder,
)


class block_scope_builder(BlockScopeBuilder):
    """
    Decorator which allows functions-returning-functions to fit the BlockScopeBuilder typeclass.

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
    """

    func: Callable[[BlockScope], Optional[Block]]

    def __init__(self, func: Callable[[BlockScope], Optional[Block]]) -> None:
        self.func = func

    def build_from_blocks(self, b: BlockScope) -> Optional[Block]:
        return self.func(b)


class inline_scope_builder(InlineScopeBuilder):
    """
    Decorator which ensures functions fit the InlineScopeBuilder typeclass

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
    """

    func: Callable[[InlineScope], Inline]

    def __init__(self, func: Callable[[InlineScope], Inline]) -> None:
        self.func = func

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        return self.func(inls)


class raw_scope_builder(RawScopeBuilder):
    """
    Decorator which allows functions to fit the RawScopeBuilder typeclass.

    e.g. one could define a function
    ```python
    def math(name=""):
        @raw_scope_builder
        def inner(raw_text: str) -> Inline:
            return ...
        return inner
    ```
    which allows turnip-text as so:
    ```!text
    [math()]#{\sin\(x\)}#
    ```
    """

    func: Callable[[str], Union[Block, Inline]]

    def __init__(self, func: Callable[[str], Union[Block, Inline]]) -> None:
        self.func = func

    def build_from_raw(self, raw: str) -> Union[Block, Inline]:
        return self.func(raw)
