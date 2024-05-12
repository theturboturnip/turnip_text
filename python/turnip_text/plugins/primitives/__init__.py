import abc
from dataclasses import dataclass
from typing import Any, Dict, Sequence, Union

import turnip_text
from turnip_text import (
    Block,
    BlockScope,
    Header,
    Inline,
    InlineScope,
    Text,
    TurnipTextSource,
)
from turnip_text.build_system import RelPath
from turnip_text.env_plugins import DocEnv, EnvPlugin, in_doc
from turnip_text.helpers import (
    NullBuilder,
    PassthroughBuilder,
    UserBlockOrInlineScopeBuilder,
    UserRawScopeBuilder,
)


@dataclass(frozen=True)
class PageBreak(Block):
    double: bool = False

    def __call__(self, /, double: bool = False) -> Any:
        return PageBreak(double)


class PrimitivesPlugin(abc.ABC, EnvPlugin):
    """Provides a set of helpful baseline features, mostly renderer-agnostic."""

    # TODO conditional subfile?
    @in_doc
    def subfile(self, doc_env: DocEnv, project_relative_path: str) -> TurnipTextSource:
        """Emit a subfile into the document, which will immediately be parsed before the rest of the current file."""
        return doc_env.build_sys.resolve_turnip_text_source(
            RelPath(project_relative_path)
        )

    # Helpers for inserting common but unwieldy unicode characters
    nbsp = Text("\u00A0")
    endash = Text("\u2013")
    emdash = Text("\u2014")

    pagebreak = PageBreak()
    """A block that implements a page break in the given language if one exists.
    
    Can be called with (double=True) to make the page break a double-break if supported.
    Double-breaks break *if needed* in a two-page setup such that the next page is the second of a pair.
    e.g. if reading left-to-right, after a double-break the next content will always be on the *right* page."""

    captured: Dict[str, Union[Block, Inline]]

    @in_doc
    def capture(self, doc_env: DocEnv, name: str = "") -> UserBlockOrInlineScopeBuilder:
        """
        Captures the following Block or Inline scope and saves it.
        The item can be `retrieve()`-d later.

        If called with a name e.g. `capture(name='fred')`, the same name must be used calling `retrieve(name='fred')`.
        Calling `capture` with the same name will overwrite the saved item.

        If not called with a name, the item is saved into a 'most-recent' slot.
        Calling `capture` with no name will overwrite the saved item.
        This is equivalent to using an empty name `capture(name="")`.
        """
        plugin = self

        class Capturer(UserBlockOrInlineScopeBuilder):
            def build_from_blocks(self, blks: BlockScope) -> None:
                plugin.captured[name] = blks
                return None

            def build_from_inlines(self, inls: InlineScope) -> None:
                plugin.captured[name] = inls
                return None

        return Capturer()

    @in_doc
    def retrieve(
        self, doc_env: DocEnv, name: str = "", keep: bool = False
    ) -> Block | Inline | None:
        """
        Retrieve an item saved with `capture()`.

        If no item has been saved with the given name, raises AttributeError.
        By default the retrieved item is forgotten, passing `keep=True` disables this behaviour.
        """
        val = self.captured[name]
        if not keep:
            del self.captured[name]
        return val

    def if_cond(self, cond: bool) -> UserBlockOrInlineScopeBuilder:
        """
        If the condition evaluates to true, returns a builder which immediately
        passes through blocks and inline scopes given.
        Otherwire, returns a builder which always returns None.
        Note that any code inside the passed-in scope will be run regardless of the condition.
        """
        return PassthroughBuilder() if cond else NullBuilder()

    @abc.abstractmethod
    def raw(self, lang: str, **kwargs: Any) -> UserRawScopeBuilder:
        """
        If the document is being rendered in the given language, emit the raw content directly into the output.
        kwargs are included to communicate extra information to specific languages, and arbitrary kwargs should never cause type errors
        """
        ...

    def __init__(self) -> None:
        super().__init__()
        self.captured = {}

    def _doc_nodes(self) -> Sequence[type[Block] | type[Inline] | type[Header]]:
        return (PageBreak,)

    # Include turnip_text as tt
    # (effectively `import turnip_text as tt`)
    tt = turnip_text
