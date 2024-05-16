import abc
import functools
import inspect
from typing import (
    Any,
    Callable,
    Dict,
    Generic,
    Optional,
    ParamSpec,
    Tuple,
    TypeVar,
    Union,
)

from turnip_text import (
    Block,
    Blocks,
    CoercibleToBlocks,
    CoercibleToInline,
    CoercibleToInlineScope,
    DocElement,
    Inline,
    InlineScope,
    Paragraph,
    Raw,
    Sentence,
    coerce_to_blocks,
    coerce_to_inline,
    coerce_to_inline_scope,
)

# TODO tests for the helpers

# TODO Python 3.12 use default here
TElement = TypeVar("TElement", bound=Optional[DocElement])


class UserBlockScopeBuilder(abc.ABC, Generic[TElement]):
    """
    Subclassable BlocksBuilder which implements the matmul operator '@'.
    Using matmul allows code to use the block scope builder more conveniently.
    It tries to coerce the right-hand-side into a Blocks before passing it to build_from_blocks.

    Example:

    `SomeUserBlockScopeBuilder() @ "Some content"` coerces "some content" into a Blocks surrounding a Paragraph before using it.
    """

    @abc.abstractmethod
    def build_from_blocks(self, blocks: Blocks) -> TElement: ...

    def __matmul__(self, maybe_b: CoercibleToBlocks) -> TElement:
        bs = coerce_to_blocks(maybe_b)
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
    def build_from_inlines(self, inls: InlineScope) -> TElement: ...

    def __matmul__(self, maybe_inls: CoercibleToInlineScope) -> TElement:
        inls = coerce_to_inline_scope(maybe_inls)
        return self.build_from_inlines(inls)


class UserRawScopeBuilder(abc.ABC, Generic[TElement]):
    """
    Subclassable RawScopeBuilder which implements the matmul operator '@'.
    Using matmul allows code to use the block scope builder more conveniently.
    It checks the right-hand side is a Raw before using it.

    Example:

    `SomeUserRawScopeBuilder() @ Raw("Some content")` performs the typecheck that the Raw is a Raw.

    For simplicity, 'str' is also supported - but beware that this won't work on combined Inline/Raw builders,
    which will coerce it to Text
    """

    @abc.abstractmethod
    def build_from_raw(self, r: Raw) -> TElement: ...

    def __matmul__(self, maybe_raw: Any) -> TElement:
        if isinstance(maybe_raw, Raw):
            return self.build_from_raw(maybe_raw)
        if isinstance(maybe_raw, str):
            return self.build_from_raw(Raw(maybe_raw))
        raise TypeError(
            f"Invoked UserRawScopeBuilder on {maybe_raw}, which wasn't a string"
        )


class UserBlockOrInlineScopeBuilder(
    UserBlockScopeBuilder[TElement], UserInlineScopeBuilder[TElement]
):
    """
    Subclassable block and inline scope builder which implements the matmul operator.
    If the argument to the matmul operator is coercible to inline, treats it as an inline.
    Otherwise tries to coerce to block.

    Example:

    `SomeUserBlockOrInlineBuilder() @ "some content"` calls build_from_inlines with "some content" coerced to Text.

    `SomeUserBlockOrInlineBuilder() @ CustomBlock()` calls build_from_blocks with Blocks([CustomBlock()])
    """

    def __matmul__(
        self, maybe_inls: Union[CoercibleToInlineScope, CoercibleToBlocks]
    ) -> TElement:
        try:
            inl = coerce_to_inline_scope(maybe_inls)  # type:ignore
        except TypeError:
            # Wasn't an inline, may be a block
            blk = coerce_to_blocks(maybe_inls)
            return self.build_from_blocks(blk)
        else:
            return self.build_from_inlines(inl)


class UserAnyScopeBuilder(
    UserBlockOrInlineScopeBuilder[TElement], UserRawScopeBuilder[TElement]
):
    """
    Subclassable block, inline, and raw scope builder which implements the matmul operator.
    If the argument to the matmul operator is Raw, treats it as Raw.
    Otherwise if the argument is coercible to inline, treats it as an inline.
    Otherwise tries to coerce to block.

    Example:

    `SomeUserAnyScopeBuilder() @ Raw("raw content")` calls build_from_raw.

    `SomeUserAnyScopeBuilder() @ "some content"` calls build_from_inlines with "some content" coerced to Text.

    `SomeUserAnyScopeBuilder() @ CustomBlock()` calls build_from_blocks with Blocks([CustomBlock()]).
    """

    def __matmul__(
        self, something: Union[Raw, CoercibleToInlineScope, CoercibleToBlocks]
    ) -> TElement:
        if isinstance(something, Raw):
            return self.build_from_raw(something)
        return super().__matmul__(something)


class PassthroughBuilder(UserBlockOrInlineScopeBuilder[Union[Block, Inline]]):
    """Block-or-inline scope builder that passes through whatever argument it's given."""

    def build_from_blocks(self, blocks: Blocks) -> Block:
        return blocks

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        return inls


class PassthroughRawBuilder(UserRawScopeBuilder):
    """Raw scope builder that passes through whatever argument it's given."""

    def build_from_raw(self, raw: Raw) -> Inline:
        return raw


class NullBuilder(UserBlockOrInlineScopeBuilder):
    """Block-or-inline scope builder that always returns None"""

    def build_from_blocks(self, _: Blocks) -> None:
        return None

    def build_from_inlines(self, _: InlineScope) -> None:
        return None


class NullRawBuilder(UserRawScopeBuilder):
    """Raw scope builder that always returns None"""

    def build_from_raw(self, _: Raw) -> None:
        return None


class block_scope_builder(UserBlockScopeBuilder[TElement]):
    """
    Decorator which allows a function to fit the BlocksBuilder typeclass.

    e.g. one could define a function
    ```python
    def block(name=""):
        @block_scope_builder
        def inner(blocks: Blocks) -> Block:
            return items
        return inner
    ```
    which allows turnip_text as so:
    ```!text
    [block(name="greg")]{
    The contents of greg
    }
    ```

    It also supports the matmul operator, which tries to coerce the right-hand-side into a Blocks before calling the function:
    ```python
    block(name="greg") @ "The contents of greg"
    ```
    """

    func: Callable[[Blocks], TElement]

    def __init__(self, func: Callable[[Blocks], TElement]) -> None:
        self.func = func
        functools.update_wrapper(self, func)

    def build_from_blocks(self, blocks: Blocks) -> TElement:
        return self.func(blocks)

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

    def build_from_inlines(self, inls: InlineScope) -> TElement:
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

    def __init__(self, func: Callable[[Raw], TElement]) -> None:
        self.func = func
        functools.update_wrapper(self, func)

    def build_from_raw(self, raw: Raw) -> TElement:
        return self.func(raw)

    def __str__(self) -> str:
        return f"<{self.__class__.__name__} wrapping {self.func}>"


P = ParamSpec("P")


# TODO replace {block,inline,raw}_scope_builder with {blocks,inlines,raw}_builder like this
# Sadly there isn't a way to make how this works obvious in the type system
class blocks_builder(Generic[P, TElement], UserBlockScopeBuilder[TElement]):
    """
    Annotates a function that takes at least one parameter `blocks: Blocks`
    to become a BlocksBuilder.

    Using directly as a BlocksBuilder will call the function with just `blocks`

    ```ttext
    [-
    @blocks_builder
    def fred(blocks):
        return blocks
    -]

    [fred]{
        Stuff
    }
    # equivalent to calling fred(blocks=Blocks([stuff]))
    ```

    Calling before using as a BlocksBuilder will call the function with those arguments and `blocks`
    ```ttext
    [-
    @blocks_builder
    def titled(title: str, blocks):
        return Blocks([
            header(title),
            blocks
        ])
    -]

    [titled("Titular Block")]{
        Stuff
    }
    # equivalent to calling [titled("Titular Block", blocks=Blocks([stuff]))]
    """

    func: Callable[P, TElement]
    args: Tuple[Any, ...]
    kwds: Dict[str, Any]

    def __init__(self, func: Callable[P, TElement]) -> None:
        super().__init__()
        if "blocks" not in inspect.signature(func).parameters:
            raise ValueError(
                f"Cannot wrap {func} in @blocks_builder, it doesn't take a 'blocks' parameter."
            )
        self.func = func
        self.args = ()
        self.kwds = {}
        functools.update_wrapper(self, func)

    def __call__(self, *args: Any, **kwds: Any) -> UserBlockScopeBuilder[TElement]:
        self.args = args
        self.kwds = kwds
        return self

    def build_from_blocks(self, blocks: Blocks) -> TElement:
        self.kwds["blocks"] = blocks
        return self.func(*self.args, **self.kwds)


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
