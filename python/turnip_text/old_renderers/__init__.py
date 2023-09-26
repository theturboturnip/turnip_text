import abc
import inspect
import os
from contextlib import contextmanager
from io import StringIO
from typing import (
    Any,
    Callable,
    Concatenate,
    Dict,
    Generator,
    Generic,
    Iterable,
    Iterator,
    List,
    Optional,
    ParamSpec,
    Self,
    Sequence,
    Tuple,
    Type,
    TypedDict,
    TypeVar,
    Union,
)

from turnip_text import (
    Block,
    BlockScope,
    Inline,
    InlineScope,
    Paragraph,
    Sentence,
    UnescapedText,
    parse_file_native,
)

# A renderer iterates depth-first through a tree of Blocks and emits text into a buffer before writing it to a file.
# The capabilities of the renderer reflect the capabilities of the format, and different formats have different capabilities.
# Thus we need plugins to extend behaviour differently for different renderers, and to allow user-generated code.

# Plugins can have stateful or stateless functions.
# Example of a stateful plugin: a TODO list which tracks and re-states the TODOs for a file at the end, anything that creates new labels.
# Example of a stateless plugin: inline formatting, simple shortcut macros
# In either case, the plugin may want to create new renderable items, and thus may be renderer-specific - a bibliography may use a LaTeX bibtex backend or manually compute citation text for Markdown, and an inline formatter will use different Latex or Markdown primitives for formatting.

# It can be useful to use stateless functions *while* rendering, once the document layout has frozen.
# e.g. when rendering a TODO item, using `ctx.bold @ ctx.color(red) @ item_text` is more convenient then directly emitting format primitives.
# This is safe because those plugins are stateless, but e.g. creating a label inside the render function may result in unexpected behaviour.

TRenderer = TypeVar("TRenderer", bound="Renderer")

TBlock = TypeVar("TBlock", bound=Block)
TInline = TypeVar("TInline", bound=Inline)


class CustomEmitDispatch(Generic[TRenderer]):
    # mypy doesn't let us use TBlock/TInline here - they're not bound to anything
    _block_table: Dict[
        Type[Block], Callable[[TRenderer, "StatelessContext[TRenderer]", Block], None]
    ]
    _inline_table: Dict[
        Type[Inline], Callable[[TRenderer, "StatelessContext[TRenderer]", Inline], None]
    ]

    def __init__(self) -> None:
        super().__init__()
        self._block_table = {}
        self._inline_table = {}

    def add_custom_block(
        self,
        t: Type[TBlock],
        f: Callable[[TRenderer, "StatelessContext[TRenderer]", TBlock], None],
    ) -> None:
        if t in self._block_table:
            raise RuntimeError(f"Conflict: registered two renderers for {t}")
        # We know that we only assign _block_table[t] if f takes t, and that when we pull it
        # out we will always call f with something of type t.
        # mypy doesn't know that, so we say _block_table stores functions taking Block (the base class)
        # and sweep the difference under the rug
        self._block_table[t] = f # type: ignore

    def add_custom_inline(
        self,
        t: Type[TInline],
        f: Callable[[TRenderer, "StatelessContext[TRenderer]", TInline], None],
    ) -> None:
        if t in self._inline_table:
            raise RuntimeError(f"Conflict: registered two renderers for {t}")
        # as above
        self._inline_table[t] = f # type: ignore

    def emit_block(
        self, renderer: TRenderer, ctx: "StatelessContext[TRenderer]", obj: TBlock
    ) -> None:
        f = self._block_table.get(type(obj))
        if f is None:
            for t, f in self._block_table.items():
                if isinstance(obj, t):
                    f(renderer, ctx, obj)
                    return
            raise NotImplementedError(f"Couldn't handle {obj}")
        else:
            f(renderer, ctx, obj)

    def emit_inline(
        self, renderer: TRenderer, ctx: "StatelessContext[TRenderer]", obj: TInline
    ) -> None:
        f = self._inline_table.get(type(obj))
        if f is None:
            for t, f in self._inline_table.items():
                if isinstance(obj, t):
                    f(renderer, ctx, obj)
                    return
            raise NotImplementedError(f"Couldn't handle {obj}")
        else:
            f(renderer, ctx, obj)


TPlugin = TypeVar("TPlugin", bound="Plugin[Any]")
P = ParamSpec("P")
T = TypeVar("T")


class Plugin(Generic[TRenderer]):
    # Initialized when the plugin is included into the MutableState.
    # Should always be non-None when the plugin's emitted functions are called
    __state: "MutableState[TRenderer]" = None  # type: ignore
    __ctx: "StatelessContext[TRenderer]" = None  # type: ignore

    def __init_ctx(
        self, state: "MutableState[TRenderer]", ctx: "StatelessContext[TRenderer]"
    ) -> None:
        assert self.__state is None and self.__ctx is None
        self.__state = state
        self.__ctx = ctx

    @property
    def _plugin_name(self) -> str:
        return type(self).__name__

    def _interface(self) -> Dict[str, Any]:
        """
        Define the interface available to the renderer context,
        and thus all eval-brackets in evaluated documents.

        By default, finds all public variables, properties (as `property` objects, without evaluating them),member functions, and static functions.
        Ignores any fields that begin with _.
        Based on https://github.com/python/cpython/blob/a0773b89dfe5cd2190d539905dd89e7f6455668e/Lib/inspect.py#L562C5-L562C5.

        May be overridden."""

        interface = {}
        names = dir(self)
        # Ignore "DynamicClassAttributes" for now, `self` isn't a class so doesn't have them
        for key in names:
            if key.startswith("_"):
                continue
            value = inspect.getattr_static(self, key)
            if isinstance(value, property):
                # Hack to make @property @stateless work.
                stateless = getattr(value.fget, "_stateless", False)
                interface[key] = BoundProperty(self, value, stateless)
            elif inspect.ismethoddescriptor(value) or inspect.isdatadescriptor(value):
                # We want to pass these through to the state/less context objects directly so they are re-__get__ted each time, but they still need to be bound to this plugin object.
                # The solution: construct BoundProperty, which binds the internal __get__, __set__, __delete__ to self,
                # and pass that through as the interface.
                stateless = getattr(value, "_stateless", False)
                interface[key] = BoundProperty(self, value, stateless)
            else:
                # Use getattr to do everything else as normal e.g. bind methods to the object etc.
                interface[key] = getattr(self, key)

        return interface

    def _add_emitters(self, handler: CustomEmitDispatch[TRenderer]) -> None:
        """
        Add render handler functions for all custom Blocks and Inlines this plugin uses
        """
        ...

    # TODO improve/remove amble handlers - these are better suited with custom blocks
    # They could be improved by making them operate on the document tree directly?
    # Maybe instead of being called on render, they're called in a specific order with MutableState before and after parsing the tree and they return Block.
    # This would mean they can return sections etc!

    def _preamble_handlers(self) -> Iterable[Tuple[str, Callable[[TRenderer], None]]]:
        return ()

    def _postamble_handlers(self) -> Iterable[Tuple[str, Callable[[TRenderer], None]]]:
        return ()

    @staticmethod
    def _make_contexts(
        plugins: Sequence["Plugin[TRenderer]"],
    ) -> Tuple["StatelessContext[TRenderer]", "MutableState[TRenderer]"]:
        ctx: "StatelessContext[TRenderer]" = StatelessContext()
        state: "MutableState[TRenderer]" = MutableState()
        for plugin in plugins:
            i = plugin._interface()

            # Everything may or may not be stateful, therefore the stateful context can include anything
            state.__dict__.update(i)

            for key, value in i.items():
                # The stateless context only includes:
                # - properties or other data descriptors, functions, and methods that have been explicitly marked stateless.
                # - class variables which aren't data descriptors (they don't have access to the plugin self and don't have access to __state.)

                is_stateless = False
                if inspect.ismethod(value):
                    is_stateless = getattr(value.__func__, "_stateless", False)
                elif hasattr(value, "__call__") or hasattr(value, "__get__"):
                    # Callables or data descriptors need to be explicitly marked as stateless
                    is_stateless = getattr(value, "_stateless", False)
                else:
                    # It's not a bound method, it's not callable or gettable, it's probably a plain variable.
                    # It can't get access to __state, so expose it.
                    is_stateless = True

                if is_stateless:
                    ctx.__dict__[key] = value
            plugin.__init_ctx(state, ctx)
        return ctx, state

    @staticmethod
    def _stateful(
        f: Callable[Concatenate[TPlugin, "MutableState[TRenderer]", P], T]
    ) -> Callable[Concatenate[TPlugin, P], T]:
        """
        An annotation for plugin bound methods which access the __state object i.e. other stateful (and stateless) functions and variables.
        This is the only way to access the state, and thus theoretically the only way to mutate state.
        Unfortunately, we can't protect a plugin from modifying its private state in a so-annotated "stateless" function.
        """

        def wrapper(plugin: TPlugin, /, *args: Any, **kwargs: Any) -> T:
            if plugin.__state._frozen:
                raise RuntimeError(
                    "Can't run a stateful function when the state is frozen!"
                )
            return f(plugin, plugin.__state, *args, **kwargs)

        wrapper._stateful = True  # type: ignore
        wrapper.__doc__ = f.__doc__

        return wrapper

    @staticmethod
    def _stateful_property(
        f: Callable[Concatenate[TPlugin, "MutableState[TRenderer]", P], T]
    ) -> property:
        """
        An example of stateful property:

        @stateful_property
        def todo(self, state: MutableState) -> InlineScopeBuilder:
            @inline_scope_builder
            def something_that_mutates_state_by_adding_a_todo(...):
                ...
            return something_that_mutates_state_by_adding_a_todo

        state is always checked to not be frozen when the property is accessed.

        In this example, the lambda which captures __state may live longer until state is frozen.
        It's your responsibility to make sure it doesn't mutate __state in that case!!

        Note that because property objects have opaque typing, typecheckers can't tell that this
        returns something of type T. This means if you try to use stateful_property to decorate a function
        that must match some property defined elsewhere, it won't work.
        In those cases, you can use @property above @stateful and it all works fine.
        """

        return property(Plugin._stateful(f))

    @staticmethod
    def _stateless(
        f: Callable[Concatenate[TPlugin, "StatelessContext[TRenderer]", P], T]
    ) -> Callable[Concatenate[TPlugin, P], T]:
        """
        An annotation for plugin bound methods which access the __ctx object i.e. other stateless functions.
        This is the only way to access ctx.
        Unfortunately, we can't protect a plugin from modifying its private state in a so-annotated "stateless" function.

        class SomePlugin(Plugin):
            @stateless
            def some_stateless_function(self, ctx, other_arg):
                return (ctx.bold @ other_arg)

        some_plugin.some_stateless_function(other_arg) # returns (ctx.bold @ other_arg), don't need to pass in ctx!

        TODO could make this pass through stuff and just set _stateless, if __ctx is changed to _ctx
        """

        def wrapper(plugin: TPlugin, /, *args: Any, **kwargs: Any) -> T:
            return f(plugin, plugin.__ctx, *args, **kwargs)

        wrapper._stateless = True  # type: ignore
        wrapper.__doc__ = f.__doc__

        return wrapper

    @staticmethod
    def _stateless_property(
        f: Callable[Concatenate[TPlugin, "StatelessContext[TRenderer]", P], T]
    ) -> property:
        """
        In case you want to use the StatelessContext in a property:

        @stateless_property
        def bold_or_italic(self, ctx):
            if random.randint(0, 1) == 1:
                return ctx.bold
            else:
                return ctx.italic

        Note that because property objects have opaque typing, typecheckers can't tell that this
        returns something of type T. This means if you try to use stateless_property to decorate a function
        that must match some property defined elsewhere, it won't work.
        In those cases, you can use @property above @stateless and it all works fine.
        """

        return property(Plugin._stateless(f))

# TODO: annoyingly, vscode doesn't auto-import these. Why?
stateful = Plugin._stateful
stateful_property = Plugin._stateful_property
stateless = Plugin._stateless
stateless_property = Plugin._stateless_property


class BoundProperty:
    "Emulate PyProperty_Type() in Objects/descrobject.c, as seen in https://docs.python.org/3.8/howto/descriptor.html"

    _obj_for_data_descriptor: Any
    _data_descriptor: Any
    _stateless: bool

    def __init__(self, obj: Any, data_descriptor: Any, stateless: bool) -> None:
        self._obj_for_data_descriptor = obj
        self._data_descriptor = data_descriptor
        self._stateless = stateless
        self.__doc__ = data_descriptor.__doc__

    def __get__(self, _obj: Any, _ownerclass: Optional[type] = None) -> Any:
        return self._data_descriptor.__get__(self._obj_for_data_descriptor)

    def __set__(self, _obj: Any, value: Any) -> None:
        self._data_descriptor.__set__(self._obj_for_data_descriptor, value)

    def __delete__(self, _obj: Any) -> None:
        self._data_descriptor.__delete__(self._obj_for_data_descriptor)


class StatelessContext(Generic[TRenderer]):
    def __getattr__(self, name: str) -> Any:
        # The StatelessContext has various things that we don't know at type-time.
        # We want to be able to use those things from Python code.
        # __getattr__ is called when an attribute access is attempted and the "normal routes" fail.
        # Defining __getattr__ tells the type checker "hey, this object has various dynamic attributes"
        # and the type checker will then allow Python code to use arbitrary attributes on this object.
        # We still need to raise AttributeError here, because we don't actually define any attributes this way.
        raise AttributeError(name=name, obj=self)


class MutableState(Generic[TRenderer]):
    _frozen: bool = False  # Set to True when rendering the document, which disables functions annotated with @stateful.

    def parse_file(self, path: Union[str, bytes, "os.PathLike[Any]"]) -> BlockScope:
        # TODO this should include a field `doc` which is itself, so longer code blocks can use properties.
        # e.g. right now if you do this then to use bound properties you have to do `footnote.__get__(None) @ blah` inside the document
        return parse_file_native(str(path), self.__dict__)
    
    def __getattr__(self, name: str) -> Any:
        # The StatelessContext has various things that we don't know at type-time.
        # We want to be able to use those things from Python code.
        # __getattr__ is called when an attribute access is attempted and the "normal routes" fail.
        # Defining __getattr__ tells the type checker "hey, this object has various dynamic attributes"
        # and the type checker will then allow Python code to use arbitrary attributes on this object.
        # We still need to raise AttributeError here, because we don't actually define any attributes this way.
        raise AttributeError(name=name, obj=self)


# TODO Make preamble/postamble return Blocks to be rendered instead of just str? Would allow e.g. a Bibliography section? Perhaps better to expose a bibliography block for "standard" postambles?
class AmbleMap(Generic[TRenderer]):
    """Class that stores and reorders {pre,post}amble handlers.

    Handlers are simply functions that return a string, which have a unique ID.

    Only one handler per ID can exist.

    The user can request that the ID are sorted in a particular order, or default to order of insertion.

    When the document is rendered, the handlers will be called in that order."""

    _handlers: Dict[str, Callable[[TRenderer], None]]
    _id_order: List[str]

    def __init__(self) -> None:
        self._handlers = {}
        self._id_order = []

    def push_handler(self, id: str, f: Callable[[TRenderer], None]) -> None:
        if id in self._handlers:
            raise RuntimeError(f"Conflict: registered two amble-handlers for ID {id}")
        self._handlers[id] = f
        self._id_order.append(id)

    def reorder_handlers(self, selected_id_order: List[str]) -> None:
        """Request that certain handler IDs are rendered in a specific order.

        Does not need to be a complete ordering, i.e. if handlers ['a', 'b', 'c'] are registered
        this function can be called with ['c', 'a'] to ensure 'c' comes before 'a',
        but all of the IDs in the order need to have been registered.

        When the requested ordering is incomplete, handlers which haven't been mentioned
        will retain their old order but there is no specified ordering between (mentioned) and (not-mentioned) IDs.
        """

        assert all(id in self._handlers for id in selected_id_order)

        if len(selected_id_order) != len(set(selected_id_order)):
            raise RuntimeError(
                f"reorder_handlers() called with ordering with duplicate IDs: {selected_id_order}"
            )

        # Shortcut if the selected order is complete i.e. covers all IDs so far
        if len(self._id_order) == len(selected_id_order):
            self._id_order = selected_id_order
        else:
            # Otherwise, we need to consider the non-selected IDs too.
            # The easy way: put selected ones first, then non-selected ones last
            # Get the list of ids NOT in selected_id_order, in the order they're currently in in self._id_order
            non_selected_ids = [
                id for id in self._id_order if id not in selected_id_order
            ]
            self._id_order = selected_id_order + non_selected_ids

        assert all(id in self._id_order for id in self._handlers.keys())

    def generate_ambles(self, renderer: TRenderer) -> Generator[Callable[[TRenderer], None], None, None]:
        for id in self._id_order:
            yield self._handlers[id]


class Renderer(abc.ABC):
    plugins: List[Plugin[Self]]
    _ctx: StatelessContext[Self]
    _state: MutableState[Self]

    emit_dispatch: CustomEmitDispatch[Self]
    preamble_handlers: AmbleMap[Self]
    postamble_handlers: AmbleMap[Self]

    document: StringIO

    _indent: str = ""
    # After emitting a newline with emit_newline, this is set.
    # The next call to emit_raw will emit _indent.
    # This is important if you want to change the indent after something has already emitted a newline,
    # e.g. if you wrap emit_paragraph() in indent(4), the emit_paragraph() will emit a final newline but *not* immediately emit the indent of 4, so subsequent emissions are nicely indented.
    # In the same way, if you emit a newline *then* change the indent, the next emitted item will have the new indent applied.
    _need_indent: bool = False

    # TODO maybe this needs to be covariant in self?
    def __init__(self: Self, plugins: Sequence[Plugin[Self]]) -> None:
        super().__init__()

        # Create render handlers and pre/postamble handlers
        self.emit_dispatch = CustomEmitDispatch()
        self.emit_dispatch.add_custom_block(
            BlockScope, lambda r, ctx, bs: r.emit_blockscope(bs)
        )
        self.emit_dispatch.add_custom_block(
            Paragraph, lambda r, ctx, bs: r.emit_paragraph(bs)
        )
        self.emit_dispatch.add_custom_inline(
            InlineScope, lambda r, ctx, inls: r.emit_inlinescope(inls)
        )
        self.emit_dispatch.add_custom_inline(
            UnescapedText, lambda r, ctx, t: r.emit_unescapedtext(t)
        )

        self.preamble_handlers = AmbleMap()
        self.postamble_handlers = AmbleMap()

        self.plugins = list(plugins)
        self._ctx, self._state = Plugin._make_contexts(self.plugins)
        for p in self.plugins:
            p._add_emitters(self.emit_dispatch)
            for preamble_id, preamble_func in p._preamble_handlers():
                self.preamble_handlers.push_handler(preamble_id, preamble_func)
            for postamble_id, postamble_func in p._postamble_handlers():
                self.postamble_handlers.push_handler(postamble_id, postamble_func)

        self.document = StringIO()

    def request_preamble_order(self, preamble_id_order: List[str]) -> None:
        self.preamble_handlers.reorder_handlers(preamble_id_order)

    def request_postamble_order(self, postamble_id_order: List[str]) -> None:
        self.postamble_handlers.reorder_handlers(postamble_id_order)

    def parse_file(self, p: "PathLike[Any]") -> BlockScope:
        return self._state.parse_file(p)

    def emit_raw(self, x: str) -> None:
        """
        The function on which all emitters are based.
        """
        if self._need_indent:
            self.document.write(self._indent)
            self._need_indent = False
        self.document.write(x)

    def emit_newline(self) -> None:
        self.document.write("\n")
        self._need_indent = True

    # TODO pass a generator instead of emit_t, ts!
    def emit_join(self, emit_t: Callable[[T], None], ts: Iterable[T], emit_join: Callable[[], None]) -> None:
        first = True
        for t in ts:
            if not first:
                emit_join()
            first = False
            emit_t(t)

    def emit_join_gen(self, emit_gen: Generator[None, None, None], emit_join: Callable[[], None]) -> None:
        first = True
        while True:
            if not first:
                emit_join()
            first = False
            try:
                next(emit_gen)
            except StopIteration:
                break
    
    def emit_break_sentence(self) -> None:
        self.emit_newline()
    
    def emit_break_paragraph(self) -> None:
        self.emit_newline()
        self.emit_newline()

    @abc.abstractmethod
    def emit_unescapedtext(self, t: UnescapedText) -> None:
        """
        Given some text, emit a string that will look like that text exactly in the given backend.
        """
        raise NotImplementedError(f"Need to implement emit_unescapedtext")

    def render_doc(self: Self, doc_block: BlockScope) -> StringIO:
        for emit_preamble in self.preamble_handlers.generate_ambles(self):
            emit_preamble(self)
            self.emit_break_paragraph()
        self.emit_blockscope(doc_block)
        for emit_postamble in self.postamble_handlers.generate_ambles(self):
            self.emit_break_paragraph()
            emit_postamble(self)
        return self.document

    # TODO this is probably a bad idea to implement because it will get mixed up with raw.
    # def emit(self, x: Any) -> None:
    #     if isinstance(x, Inline):
    #         self.emit_inline(x)
    #     else:
    #         self.emit_block(x)


    # TODO or i could get even crazier with it - make it expand tuples?
    def emit(self, *args: Any, joiner: Optional[Callable[[], None]]=None) -> None:
        first = True            
        for a in args:
            if joiner and not first:
                joiner()
            first = False
            if isinstance(a, str):
                self.emit_raw(a)
            elif isinstance(a, Inline):
                self.emit_inline(a)
            else:
                self.emit_block(a)

    def emit_inline(self: Self, i: Inline) -> None:
        self.emit_dispatch.emit_inline(self, self._ctx, i)

    def emit_block(self: Self, b: Block) -> None:
        self.emit_dispatch.emit_block(self, self._ctx, b)

    def emit_blockscope(self, bs: BlockScope) -> None:
        # Default: join paragraphs with self.PARAGRAPH_SEP
        # If you get nested blockscopes, this will still be fine - you won't get double separators
        self.emit_join(self.emit_block, bs, self.emit_break_paragraph)

    def emit_paragraph(self, p: Paragraph) -> None:
        # Default: join sentences with self.SENTENCE_SEP
        for s in p:
            self.emit_sentence(s)

    def emit_inlinescope(self, inls: InlineScope) -> None:
        # Default: join internal inline elements directly
        for i in inls:
            self.emit_inline(i)

    def emit_sentence(self, s: Sentence) -> None:
        # Default: join internal inline elements directly
        # TODO could be extended by e.g. latex to ensure you get sentence-break-whitespace at the end of each sentence?
        for i in s:
            self.emit_inline(i)
        # TODO this shouldn't be here, surely. it should be in emit_paragraph, *joining* sentences instead of *ending* them
        self.emit_break_sentence()

    def push_indent(self, n: int) -> None:
        self._indent += " " * n

    def pop_indent(self, n: int) -> None:
        if len(self._indent) < n:
            raise ValueError()
        self._indent = self._indent[:-n]

    @contextmanager
    def indent(self, n: int) -> Iterator[None]:
        self.push_indent(n)
        try: 
            yield
        finally:
            self.pop_indent(n)
# class DocumentConfig(Generic[TRenderer], abc.ABC):
#    pass

# We want to be able to
# - add custom args to Renderer.__init__ (so we can't hardcode how Renderers are created)
# - add custom reproducible transformations to a parsed document BlockScope (to make up for a lack of ambles)
# - set a default set of plugins for documents (so we have to start all this through the DocumentConfig)