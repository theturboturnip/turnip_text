from contextlib import AbstractContextManager, contextmanager
from typing import Any, Callable, Dict, Generic, Sequence, Tuple, Type, TypeVar

from turnip_text import (
    Block,
    BlockScope,
    Inline,
    InlineScope,
    Paragraph,
    Sentence,
    UnescapedText,
)
from turnip_text.renderers.dictify import dictify

# A renderer iterates depth-first through a tree of Blocks and emits text into a buffer before writing it to a file.
# The capabilities of the renderer reflect the capabilities of the format, and different formats have different capabilities.
# Thus we need plugins to extend behaviour differently for different renderers, and to allow user-generated code.

# Plugins can either be stateful or stateless.
# Example of a stateful plugin: a TODO list which tracks and re-states the TODOs for a file at the end, anything that creates new labels.
# Example of a stateless plugin: inline formatting, simple shortcut macros
# In either case, the plugin may want to create new renderable items, and thus may be renderer-specific - a bibliography may use a LaTeX bibtex backend or manually compute citation text for Markdown, and an inline formatter will use different Latex or Markdown primitives for formatting.

# It can be useful to use stateless plugins *while* rendering, once the document layout has frozen.
# e.g. when rendering a TODO item, using `ctx.bold @ ctx.color(red) @ item_text` is more convenient then directly emitting format primitives.
# This is safe because those plugins are stateless, but e.g. creating a label inside the render function may result in unexpected behaviour.

TRenderer = TypeVar("TRenderer", bound="Renderer")

TBlock = TypeVar("TBlock", bound=Block)
TInline = TypeVar("TInline", bound=Inline)


class CustomRenderDispatch(Generic[TRenderer]):
    _block_table: Dict[Type[Block], Callable[[TRenderer, Block], None]]
    _inline_table: Dict[Type[Inline], Callable[[TRenderer, Inline], None]]

    def __init__(self) -> None:
        super().__init__()
        self._block_table = {}
        self._inline_table = {}

    def add_custom_block(self, t: Type[TBlock], f: Callable[[TRenderer, TBlock], None]):
        if t in self._block_table:
            raise RuntimeError(f"Conflict: registered two renderers for {t}")
        self._block_table[t] = f  # type: ignore

    def add_custom_inline(
        self, t: Type[TInline], f: Callable[[TRenderer, TInline], None]
    ):
        if t in self._inline_table:
            raise RuntimeError(f"Conflict: registered two renderers for {t}")
        self._inline_table[t] = f  # type: ignore

    def render_block(self, renderer: TRenderer, obj: TBlock):
        f = self._block_table.get(type(obj))
        if f is None:
            for t, f in self._block_table.items():
                if isinstance(obj, t):
                    f(renderer, obj)
                    return
            raise NotImplementedError(f"Couldn't handle {obj}")
        else:
            f(renderer, obj)

    def render_inline(self, renderer: TRenderer, obj: TInline):
        f = self._inline_table.get(type(obj))
        if f is None:
            for t, f in self._inline_table.items():
                if isinstance(obj, t):
                    f(renderer, obj)
                    return
            raise NotImplementedError(f"Couldn't handle {obj}")
        else:
            f(renderer, obj)


class StatelessPlugin(Generic[TRenderer]):
    # Initialized when the plugin is included into the StatelessContext.
    # Should always be non-None when the plugin's emitted functions are called
    _ctx: "StatelessContext[TRenderer]" = None  # type: ignore

    def __init_ctx(self, _ctx: "StatelessContext[TRenderer]"):
        self._ctx = _ctx

    @property
    def _plugin_name(self) -> str:
        return type(self).__name__

    def _interface(self) -> Dict[str, Any]:
        """Define the interface available to the renderer context,
        and thus all eval-brackets in evaluated documents.

        By default, uses dictify() to find all public variables, member functions, and static functions.

        May be overridden."""
        return dictify(self)

    def _add_renderers(self, handler: CustomRenderDispatch[TRenderer]):
        """
        Add render handler functions for all custom Blocks and Inlines this plugin uses
        """
        ...


class StatelessContext(Generic[TRenderer]):
    def __init__(self) -> None:
        super().__init__()

    @classmethod
    def make_context(
        cls: Type["StatelessContext[TRenderer]"],
        plugins: Sequence[StatelessPlugin[TRenderer]],
    ) -> "StatelessContext[TRenderer]":
        ctx = cls.__new__(cls)
        for plugin in plugins:
            # Strip things beginning with _ from plugin._interface()
            i = plugin._interface()
            for key in i.keys():
                if key.startswith("_"):
                    del i[key]
            ctx.__dict__.update(i)
            plugin._StatelessPlugin__init_ctx(ctx)  # type: ignore
        return ctx


class StatefulPlugin(Generic[TRenderer]):
    # Initialized when the plugin is included into the StatefulContext.
    # Should always be non-None when the plugin's emitted functions are called
    _state: "StatefulContext[TRenderer]" = None  # type: ignore
    _ctx: "StatelessContext[TRenderer]" = None  # type: ignore

    def __init_ctx(
        self, _state: "StatefulContext[TRenderer]", _ctx: "StatelessContext[TRenderer]"
    ):
        self._state = _state
        self._ctx = _ctx

    @property
    def _plugin_name(self) -> str:
        return type(self).__name__

    def _interface(self) -> Dict[str, Any]:
        """Define the interface available to the renderer context,
        and thus all eval-brackets in evaluated documents.

        By default, uses dictify() to find all public variables, member functions, and static functions.

        May be overridden."""
        return dictify(self)

    def _add_renderers(self, handler: CustomRenderDispatch[TRenderer]):
        """
        Add render handler functions for all custom Blocks and Inlines this plugin uses
        """
        ...


class StatefulContext(Generic[TRenderer]):
    @classmethod
    def make_context(
        cls: Type["StatefulContext[TRenderer]"],
        ctx: StatelessContext[TRenderer],
        plugins: Sequence[StatefulPlugin[TRenderer]],
    ) -> "StatefulContext[TRenderer]":
        state = cls.__new__(cls)
        for plugin in plugins:
            # Strip things beginning with _ from plugin._interface()
            i = plugin._interface()
            for key in i.keys():
                if key.startswith("_"):
                    del i[key]
            state.__dict__.update(i)
            plugin._StatefulPlugin__init_ctx(state)  # type: ignore
        return state


class Renderer:
    @classmethod
    def parse_and_render(
        cls: Type[TRenderer],
        stateless_plugins: Sequence[StatelessPlugin[TRenderer]],
        stateful_plugins: Sequence[StatefulPlugin[TRenderer]],
    ):
        pass
