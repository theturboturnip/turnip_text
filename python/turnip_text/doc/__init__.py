"""
This module
"""

import abc
import inspect
from dataclasses import dataclass
from os import PathLike
from typing import (
    Any,
    Callable,
    Concatenate,
    Dict,
    List,
    Optional,
    ParamSpec,
    Sequence,
    Set,
    Tuple,
    Type,
    TypeVar,
    Union,
)

from turnip_text import (
    Block,
    BlockScope,
    DocSegment,
    DocSegmentHeader,
    Inline,
    parse_file_native,
)

__all__ = [
    "Document",
    "parse",
    "DocPlugin",
    "DocState",
    "FormatContext",
    "anchors",
    "stateful",
    "stateful_property",
    "stateless",
    "stateless_property",
    "BoundProperty"
]

TDocPlugin = TypeVar("TDocPlugin", bound="DocPlugin")
P = ParamSpec("P")
T = TypeVar("T")

@dataclass
class Document:
    exported_nodes: Set[Type[Union[Block, Inline, DocSegmentHeader]]]
    counted_anchor_kinds: Set[str]
    fmt: "FormatContext"
    toplevel: DocSegment


def parse(path: Union[str, bytes, PathLike], plugins: Sequence["DocPlugin"]) -> Document:
    fmt, doc = DocPlugin._make_contexts(plugins)

    exported_nodes: Set[Type[Union[Block, Inline, DocSegmentHeader]]] = set()
    counters: Set[str] = set()
    for p in plugins:
        exported_nodes.update(p._doc_nodes())
        counters.update(p._countables())

    # First pass: parsing
    doc_toplevel = doc.parse_file_to_block(path)

    # Second pass: modifying the document
    for p in plugins:
        doc_toplevel = p._mutate_document(doc, fmt, doc_toplevel)

    # Now freeze the document so further passes don't mutate it.
    doc._frozen = True

    return Document(
        exported_nodes,
        counters,
        fmt,
        doc_toplevel
    )

class DocPlugin:
    # Initialized when the plugin is included into the MutableState.
    # Should always be non-None when the plugin's emitted functions are called
    __doc: "DocState" = None  # type: ignore
    __fmt: "FormatContext" = None  # type: ignore

    def __init_ctx(
        self, fmt: "FormatContext", doc: "DocState"
    ) -> None:
        assert self.__doc is None and self.__fmt is None
        self.__doc = doc
        self.__fmt = fmt

    @property
    def _plugin_name(self) -> str:
        return type(self).__name__
    
    @abc.abstractmethod
    def _doc_nodes(self) -> Sequence[Type[Union[Block, Inline, DocSegmentHeader]]]:
        """
        Tell the Document what nodes this plugin exports
        """
        return []
    
    def _countables(self) -> Sequence[str]:
        """
        Tell the Document what counters this plugin uses
        """
        return []
    
    def _mutate_document(self, doc: "DocState", fmt: "FormatContext", toplevel: DocSegment) -> DocSegment:
        """
        Mutate the toplevel_contents or toplevel_segments to add things as you please.
        You may make a copy and return it
        """
        return toplevel

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
                # We want to pass these through to the doc/less context objects directly so they are re-__get__ted each time, but they still need to be bound to this plugin object.
                # The solution: construct BoundProperty, which binds the internal __get__, __set__, __delete__ to self,
                # and pass that through as the interface.
                stateless = getattr(value, "_stateless", False)
                interface[key] = BoundProperty(self, value, stateless)
            else:
                # Use getattr to do everything else as normal e.g. bind methods to the object etc.
                interface[key] = getattr(self, key)

        return interface

    @staticmethod
    def _make_contexts(
        plugins: Sequence["DocPlugin"],
    ) -> Tuple["FormatContext", "DocState"]:
        fmt = FormatContext()
        doc = DocState(fmt)
        for plugin in plugins:
            i = plugin._interface()

            for key, value in i.items():
                # The stateless context only includes:
                # - properties or other data descriptors, functions, and methods that have been explicitly marked stateless.
                # - class variables which aren't data descriptors (they don't have access to the plugin self and don't have access to __doc.)

                if key in RESERVED_DOC_PLUGIN_EXPORTS:
                    print(f"Warning: ignoring reserved field {key} of plugin {plugin}")
                    continue

                is_stateless = False
                if inspect.ismethod(value):
                    is_stateless = getattr(value.__func__, "_stateless", False)
                elif hasattr(value, "__call__") or hasattr(value, "__get__"):
                    # Callables or data descriptors need to be explicitly marked as stateless
                    is_stateless = getattr(value, "_stateless", False)
                else:
                    # It's not a bound method, it's not callable or gettable, it's probably a plain variable.
                    # It can't get access to __doc, so expose it.
                    is_stateless = True

                if is_stateless:
                    fmt.__dict__[key] = value
                else:
                    doc.__dict__[key] = value
            plugin.__init_ctx(fmt, doc)
        return fmt, doc

    @staticmethod
    def _stateful(
        f: Callable[Concatenate[TDocPlugin, "DocState", P], T]
    ) -> Callable[Concatenate[TDocPlugin, P], T]:
        """
        An annotation for plugin bound methods which access the __doc object i.e. other stateful (and stateless) functions and variables.
        This is the only way to access the doc, and thus theoretically the only way to mutate doc.
        Unfortunately, we can't protect a plugin from modifying its private doc in a so-annotated "stateless" function.
        """

        def wrapper(plugin: TDocPlugin, /, *args: Any, **kwargs: Any) -> T:
            if plugin.__doc._frozen:
                raise RuntimeError(
                    "Can't run a stateful function when the doc is frozen!"
                )
            return f(plugin, plugin.__doc, *args, **kwargs)

        wrapper._stateful = True  # type: ignore
        wrapper.__doc__ = f.__doc__

        return wrapper

    @staticmethod
    def _stateful_property(
        f: Callable[Concatenate[TDocPlugin, "DocState", P], T]
    ) -> property:
        """
        An example of stateful property:

        @stateful_property
        def todo(self, doc: MutableState) -> InlineScopeBuilder:
            @inline_scope_builder
            def something_that_mutates_state_by_adding_a_todo(...):
                ...
            return something_that_mutates_state_by_adding_a_todo

        doc is always checked to not be frozen when the property is accessed.

        In this example, the lambda which captures __doc may live longer until doc is frozen.
        It's your responsibility to make sure it doesn't mutate __doc in that case!!

        Note that because property objects have opaque typing, typecheckers can't tell that this
        returns something of type T. This means if you try to use stateful_property to decorate a function
        that must match some property defined elsewhere, it won't work.
        In those cases, you can use @property above @stateful and it all works fine.
        """

        return property(DocPlugin._stateful(f))

    @staticmethod
    def _stateless(
        f: Callable[Concatenate[TDocPlugin, "FormatContext", P], T]
    ) -> Callable[Concatenate[TDocPlugin, P], T]:
        """
        An annotation for plugin bound methods which access the __fmt object i.e. other stateless functions.
        This is the only way to access fmt.
        Unfortunately, we can't protect a plugin from modifying its private doc in a so-annotated "stateless" function.

        class SomePlugin(Plugin):
            @stateless
            def some_stateless_function(self, fmt, other_arg):
                return (fmt.bold @ other_arg)

        some_plugin.some_stateless_function(other_arg) # returns (fmt.bold @ other_arg), don't need to pass in fmt!

        TODO could make this pass through stuff and just set _stateless, if __fmt is changed to _ctx
        """

        def wrapper(plugin: TDocPlugin, /, *args: Any, **kwargs: Any) -> T:
            return f(plugin, plugin.__fmt, *args, **kwargs)

        wrapper._stateless = True  # type: ignore
        wrapper.__doc__ = f.__doc__

        return wrapper

    @staticmethod
    def _stateless_property(
        f: Callable[Concatenate[TDocPlugin, "FormatContext", P], T]
    ) -> property:
        """
        In case you want to use the StatelessContext in a property:

        @stateless_property
        def bold_or_italic(self, fmt):
            if random.randint(0, 1) == 1:
                return fmt.bold
            else:
                return fmt.italic

        Note that because property objects have opaque typing, typecheckers can't tell that this
        returns something of type T. This means if you try to use stateless_property to decorate a function
        that must match some property defined elsewhere, it won't work.
        In those cases, you can use @property above @stateless and it all works fine.
        """

        return property(DocPlugin._stateless(f))

# TODO: annoyingly, vscode doesn't auto-import these. Why?
stateful = DocPlugin._stateful
stateful_property = DocPlugin._stateful_property
stateless = DocPlugin._stateless
stateless_property = DocPlugin._stateless_property

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


RESERVED_DOC_PLUGIN_EXPORTS = [
    "doc",
    "fmt",
]

class FormatContext:
    def __getattr__(self, name: str) -> Any:
        # The FormatContext has various things that we don't know at type-time.
        # We want to be able to use those things from Python code.
        # __getattr__ is called when an attribute access is attempted and the "normal routes" fail.
        # Defining __getattr__ tells the type checker "hey, this object has various dynamic attributes"
        # and the type checker will then allow Python code to use arbitrary attributes on this object.
        # We still need to raise AttributeError here, because we don't actually define any attributes this way.
        raise AttributeError(name=name, obj=self)


class DocState:
    _frozen: bool = False  # Set to True when rendering the document, which disables functions annotated with @stateful.

    # These are reserved fields, so plugins can't export them.
    # Evaluated code can call directly out to doc.blah or fmt.blah.
    doc: 'DocState'
    fmt: 'FormatContext'

    def __init__(self, fmt: 'FormatContext') -> None:
        self.doc = self
        self.fmt = fmt
    
    def parse_file_to_block(self, path: Union[str, bytes, PathLike]) -> DocSegment:
        return parse_file_native(str(path), self.__dict__)

    def __getattr__(self, name: str) -> Any:
        # The StatelessContext has various things that we don't know at type-time.
        # We want to be able to use those things from Python code.
        # __getattr__ is called when an attribute access is attempted and the "normal routes" fail.
        # Defining __getattr__ tells the type checker "hey, this object has various dynamic attributes"
        # and the type checker will then allow Python code to use arbitrary attributes on this object.
        # We still need to raise AttributeError here, because we don't actually define any attributes this way.
        raise AttributeError(name=name, obj=self)
