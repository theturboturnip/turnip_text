import abc
import functools
from typing import Any, Callable, Generic, List, Optional, TypeVar, Union

from turnip_text import (
    Block,
    BlockScope,
    BlockScopeBuilder,
    CoercibleToBlockScope,
    CoercibleToInline,
    CoercibleToInlineScope,
    Header,
    Inline,
    InlineScope,
    InlineScopeBuilder,
    Paragraph,
    Raw,
    RawScopeBuilder,
    Sentence,
    coerce_to_block_scope,
    coerce_to_inline,
    coerce_to_inline_scope,
)

# TODO tests for the helpers

# TODO Python 3.12 use default here
TElement = TypeVar("TElement", bound=Union[Header, Block, Inline, None])

class UserBlockScopeBuilder(abc.ABC, Generic[TElement]):
    """
    Subclassable BlockScopeBuilder which implements the matmul operator '@'.
    Using matmul allows code to use the block scope builder more conveniently.
    It tries to coerce the right-hand-side into a BlockScope before passing it to build_from_blocks.

    Example:

    `SomeUserBlockScopeBuilder() @ "Some content"` coerces "some content" into a BlockScope surrounding a Paragraph before using it.
    """

    @abc.abstractmethod
    def build_from_blocks(
        self, blks: BlockScope
    ) -> TElement: ...

    def __matmul__(
        self, maybe_b: CoercibleToBlockScope
    ) -> TElement:
        bs = coerce_to_block_scope(maybe_b)
        return self.build_from_blocks(bs)


class UserInlineScopeBuilder(abc.ABC, Generic[TElement]):
    """
    Subclassable InlineScopeBuilder which implements the matmul operator '@'.
    Using matmul allows code to use the block scope builder more conveniently.
    It tries to coerce the right-hand-side into an InlineScope before passing it to build_from_blocks.

    Example:

    `SomeUserInlineScopeBuilder() @ "Some content"` coerces "some content" into Text before using it.
    """

    @abc.abstractmethod
    def build_from_inlines(
        self, inls: InlineScope
    ) -> TElement: ...

    def __matmul__(
        self, maybe_inls: CoercibleToInlineScope
    ) -> TElement:
        inls = coerce_to_inline_scope(maybe_inls)
        return self.build_from_inlines(inls)


class UserRawScopeBuilder(abc.ABC, Generic[TElement]):
    """
    Subclassable RawScopeBuilder which implements the matmul operator '@'.
    Using matmul allows code to use the block scope builder more conveniently.
    It checks the right-hand side is a Raw before using it.

    Example:

    `SomeUserRawScopeBuilder() @ Raw("Some content")` performs the typecheck that the Raw is a Raw.
    """

    @abc.abstractmethod
    def build_from_raw(self, r: Raw) -> TElement: ...

    def __matmul__(self, maybe_raw: Any) -> TElement:
        if isinstance(maybe_raw, Raw):
            return self.build_from_raw(maybe_raw)
        raise TypeError(
            f"Invoked UserRawScopeBuilder on {maybe_raw}, which wasn't a string"
        )


class UserBlockOrInlineScopeBuilder(UserBlockScopeBuilder[TElement], UserInlineScopeBuilder[TElement]):
    """
    Subclassable block and inline scope builder which implements the matmul operator.
    If the argument to the matmul operator is coercible to inline, treats it as an inline.
    Otherwise tries to coerce to block.

    Example:

    `SomeUserBlockOrInlineBuilder() @ "some content"` calls build_from_inlines with "some content" coerced to Text.

    `SomeUserBlockOrInlineBuilder() @ CustomBlock()` calls build_from_blocks with BlockScope([CustomBlock()])
    """

    def __matmul__(
        self, maybe_inls: Union[CoercibleToInlineScope, CoercibleToBlockScope]
    ) -> TElement:
        try:
            inl = coerce_to_inline_scope(maybe_inls)  # type:ignore
        except TypeError:
            # Wasn't an inline, may be a block
            blk = coerce_to_block_scope(maybe_inls)
            return self.build_from_blocks(blk)
        else:
            return self.build_from_inlines(inl)


class UserAnyScopeBuilder(UserBlockOrInlineScopeBuilder[TElement], UserRawScopeBuilder[TElement]):
    """
    Subclassable block, inline, and raw scope builder which implements the matmul operator.
    If the argument to the matmul operator is Raw, treats it as Raw.
    Otherwise if the argument is coercible to inline, treats it as an inline.
    Otherwise tries to coerce to block.

    Example:

    `SomeUserAnyScopeBuilder() @ Raw("raw content")` calls build_from_raw.

    `SomeUserAnyScopeBuilder() @ "some content"` calls build_from_inlines with "some content" coerced to Text.

    `SomeUserAnyScopeBuilder() @ CustomBlock()` calls build_from_blocks with BlockScope([CustomBlock()]).
    """

    def __matmul__(
        self, something: Union[Raw, CoercibleToInlineScope, CoercibleToBlockScope]
    ) -> TElement:
        if isinstance(something, Raw):
            return self.build_from_raw(something)
        return super().__matmul__(something)


class PassthroughBuilder(UserBlockOrInlineScopeBuilder[Union[Block, Inline]]):
    """Block-or-inline scope builder that passes through whatever argument it's given."""

    def build_from_blocks(self, bs: BlockScope) -> Block:
        return bs

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        return inls


class PassthroughRawBuilder(UserRawScopeBuilder):
    """Raw scope builder that passes through whatever argument it's given."""

    def build_from_raw(self, raw: Raw) -> Inline:
        return raw


class NullBuilder(UserBlockOrInlineScopeBuilder):
    """Block-or-inline scope builder that always returns None"""

    def build_from_blocks(self, _: BlockScope) -> None:
        return None

    def build_from_inlines(self, _: InlineScope) -> None:
        return None


class NullRawBuilder(UserRawScopeBuilder):
    """Raw scope builder that always returns None"""

    def build_from_raw(self, _: Raw) -> None:
        return None


class block_scope_builder(UserBlockScopeBuilder[TElement]):
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
    which allows turnip_text as so:
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

    func: Callable[[BlockScope], TElement]

    def __init__(
        self, func: Callable[[BlockScope], TElement]
    ) -> None:
        self.func = func
        functools.update_wrapper(self, func)

    def build_from_blocks(self, b: BlockScope) -> TElement:
        return self.func(b)
    
    def __str__(self) -> str:
        return f"<{self.__class__.__name__} wrapping {self.func}>"

class inline_scope_builder(UserInlineScopeBuilder[TElement]):
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
    which allows turnip_text as so:
    ```!text
    [inline("!")]{surprise}
    ```

    It also supports the matmul operator, which tries to coerce the right-hand-side into an InlineScope before calling the function:
    ```python
    inline("!") @ "surprise"
    ```
    """

    func: Callable[[InlineScope], TElement]

    def __init__(
        self,
        func: Callable[[InlineScope], TElement],
    ) -> None:
        self.func = func
        functools.update_wrapper(self, func)

    def build_from_inlines(
        self, inls: InlineScope
    ) -> TElement:
        return self.func(inls)
    
    def __str__(self) -> str:
        return f"<{self.__class__.__name__} wrapping {self.func}>"

class raw_scope_builder(UserRawScopeBuilder[TElement]):
    """
    Decorator which allows a function to fit the RawScopeBuilder typeclass.

    e.g. one could define a function
    ```python
    @raw_scope_builder
    def math(raw_text: str) -> Inline:
        ...
    ```
    which allows turnip_text as so:
    ```!text
    [math]#{\\sin x}#
    ```

    It also supports the matmul operator, which checks the right-hand-side is a Raw before calling the function:
    ```python
    math @ Raw("\\sin x")
    ```
    """

    func: Callable[[Raw], TElement]

    def __init__(
        self, func: Callable[[Raw], TElement]
    ) -> None:
        self.func = func
        functools.update_wrapper(self, func)

    def build_from_raw(self, raw: Raw) -> TElement:
        return self.func(raw)
    
    def __str__(self) -> str:
        return f"<{self.__class__.__name__} wrapping {self.func}>"


def paragraph_of(i: CoercibleToInline) -> Paragraph:
    return Paragraph([Sentence([coerce_to_inline(i)])])


class Unset:
    def __eq__(self, __value: object) -> bool:
        if isinstance(__value, Unset):
            return True
        return False


UNSET = Unset()

T = TypeVar("T")
MaybeUnset = Union[T, Unset]
