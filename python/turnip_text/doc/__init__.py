"""
This module
"""

import abc
import inspect
import os
import re
from collections import defaultdict
from dataclasses import dataclass
from typing import (
    Any,
    Callable,
    Concatenate,
    Dict,
    Iterable,
    List,
    Optional,
    ParamSpec,
    Protocol,
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
    InsertedFile,
    parse_file_native,
)
from turnip_text.doc.anchors import Anchor, Backref

__all__ = [
    "Document",
    "parse_pass",
    "mutate_pass",
    "DocPlugin",
    "DocState",
    "FormatContext",
    "anchors",
    "stateful",
    "stateful_property",
    "stateless",
    "stateless_property",
    "BoundProperty",
]

TDocPlugin = TypeVar("TDocPlugin", bound="DocPlugin")
P = ParamSpec("P")
T = TypeVar("T")


class DocSetup:
    plugins: Sequence["DocPlugin"]
    fmt: "FormatContext"
    doc: "DocState"

    def __init__(self, plugins: Sequence["DocPlugin"]) -> None:
        self.plugins = plugins
        self.fmt, self.doc = DocPlugin._make_contexts(plugins)

    @property
    def anchors(self) -> "DocAnchors":
        return self.doc.anchors

    def parse(self, f: InsertedFile) -> DocSegment:
        return parse_file_native(f, self.doc.__dict__)

    def freeze(self) -> None:
        self.doc._frozen = True


class DocMutator(Protocol):
    """Both DocPlugins and RenderPlugins have the option to mutate the state of the document post-parse.

    They inherit this interface."""

    # TODO rename to exported_doc_nodes or soemthing
    def _doc_nodes(self) -> Sequence[Type[Union[Block, Inline, DocSegmentHeader]]]:
        """
        Tell the Document what nodes this plugin exports
        """
        return []

    def _mutate_document(
        self, doc: "DocState", fmt: "FormatContext", toplevel: DocSegment
    ) -> DocSegment:
        """
        Mutate the toplevel_contents or toplevel_segments to add things as you please.
        You may make a copy and return it
        """
        return toplevel

    def _countables(self) -> Sequence[str]:
        """
        Tell the Document what counters this plugin uses
        """
        return []


class DocPlugin(DocMutator):
    # Initialized when the plugin is included into the MutableState.
    # Should always be non-None when the plugin's emitted functions are called
    __doc: "DocState" = None  # type: ignore
    __fmt: "FormatContext" = None  # type: ignore

    def __init_ctx(self, fmt: "FormatContext", doc: "DocState") -> None:
        assert self.__doc is None and self.__fmt is None
        self.__doc = doc
        self.__fmt = fmt

    @property
    def _plugin_name(self) -> str:
        return type(self).__name__

    def _countables(self) -> Sequence[str]:
        """
        Tell the Document what counters this plugin uses
        """
        return []

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
        anchors = DocAnchors()
        fmt = FormatContext()
        doc = DocState(fmt, anchors)

        def register_plugin(plugin: "DocPlugin") -> None:
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

                # No matter what, the function gets added to the stateful context (even if they're stateless!)
                # If you have access to mutable state, you're allowed to call stateless functions too.
                doc.__dict__[key] = value
                # Stateless functions get added to the stateless context
                if is_stateless:
                    fmt.__dict__[key] = value

            plugin.__init_ctx(fmt, doc)

        register_plugin(anchors)
        for plugin in plugins:
            register_plugin(plugin)

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
    "anchors",
    "backref",
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
    _frozen: bool = (
        False  # Set to True when rendering the document, which disables functions annotated with @stateful.
    )

    # These are reserved fields, so plugins can't export them.
    # Evaluated code can call directly out to doc.blah or fmt.blah.
    doc: "DocState"
    fmt: "FormatContext"
    anchors: "DocAnchors"
    # This can be used by all document code to create backrefs, optionally with custom labels.
    backref = Backref

    def __init__(self, fmt: "FormatContext", anchors: "DocAnchors") -> None:
        self.doc = self
        self.fmt = fmt
        self.anchors = anchors

    def __getattr__(self, name: str) -> Any:
        # The StatelessContext has various things that we don't know at type-time.
        # We want to be able to use those things from Python code.
        # __getattr__ is called when an attribute access is attempted and the "normal routes" fail.
        # Defining __getattr__ tells the type checker "hey, this object has various dynamic attributes"
        # and the type checker will then allow Python code to use arbitrary attributes on this object.
        # We still need to raise AttributeError here, because we don't actually define any attributes this way.
        raise AttributeError(name=name, obj=self)


class DocAnchors(DocPlugin):
    """Responsible for keeping track of all the anchors in a document.

    Has enough information to convert a Backref to the Anchor that it refers to (inferring the kind)
    and retrieve information associated with the anchor.
    Allows document code to create anchors with `register_new_anchor()` or `register_new_anchor_with_float()`.
    Any backref can be converted to an Anchor (usually for rendering purposes) with `lookup_backref()`.
    The data associated with an Anchor in `register_new_anchor_with_float()` can be retrieved with an Anchor `lookup_anchor_float()` or a Backref to that Anchor `lookup_backref_float()`.

    Anchors can be created without knowing their ID, at which point this will generate an ID from a monotonic per-kind counter.
    To avoid overlap with user-defined IDs, user-defined IDs must contain at least one alphabetic latin character (upper or lowercase).

    This is a DocPlugin so that it can use the @stateful annotation to avoid creating new anchors after the document is frozen.
    """

    #

    _anchor_kind_counters: Dict[str, int]
    _anchor_id_to_possible_kinds: Dict[str, Dict[str, Anchor]]
    _anchored_floats: Dict[Anchor, Block]  # TODO rename floating_space

    # Anchor IDs, if they're user-defined, they must be
    _VALID_USER_ANCHOR_ID_REGEX = re.compile(r"\w*[a-zA-Z]\w*")

    def __init__(self) -> None:
        self._anchor_kind_counters = defaultdict(lambda: 1)
        self._anchor_id_to_possible_kinds = defaultdict(dict)
        self._anchored_floats = {}

    @stateful
    def register_new_anchor(
        self, doc: DocState, kind: str, id: Optional[str]
    ) -> Anchor:
        """
        When inside the document, create a new anchor.
        """
        if id is None:
            id = str(self._anchor_kind_counters[kind])
        else:
            # Guarantee no overlap with auto-generated anchor IDs
            assert self._VALID_USER_ANCHOR_ID_REGEX.match(
                id
            ), "User-defined anchor IDs must have at least one alphabetic character"

        if self._anchor_id_to_possible_kinds[id].get(kind) is not None:
            raise ValueError(
                f"Tried to register anchor kind={kind}, id={id} when it already existed"
            )

        l = Anchor(
            kind=kind,
            id=id,
        )
        self._anchor_kind_counters[kind] += 1
        self._anchor_id_to_possible_kinds[id][kind] = l
        return l

    def register_new_anchor_with_float(
        self,
        kind: str,
        id: Optional[str],
        float_gen: Callable[[Anchor], Block],
    ) -> Anchor:
        a = self.register_new_anchor(kind, id)
        self._anchored_floats[a] = float_gen(a)
        return a

    def lookup_backref(self, backref: Backref) -> Anchor:
        """
        Should be called by renderers to resolve a backref into an anchor.
        The renderer can then retrieve the counters for the anchor.
        """

        if backref.id not in self._anchor_id_to_possible_kinds:
            raise ValueError(
                f"Backref {backref} refers to an ID '{backref.id}' with no anchor!"
            )

        possible_kinds = self._anchor_id_to_possible_kinds[backref.id]

        if backref.kind is None:
            if len(possible_kinds) != 1:
                raise ValueError(
                    f"Backref {backref} doesn't specify the kind of anchor it's referring to, and there are multiple with that ID: {possible_kinds}"
                )
            only_possible_anchor = next(iter(possible_kinds.values()))
            return only_possible_anchor
        else:
            if backref.kind not in possible_kinds:
                raise ValueError(
                    f"Backref {backref} specifies an anchor of kind {backref.kind}, which doesn't exist for ID {backref.id}: {possible_kinds}"
                )
            return possible_kinds[backref.kind]

    def lookup_anchor_float(self, anchor: Anchor) -> Optional[Block]:
        return self._anchored_floats.get(anchor)

    def lookup_backref_float(self, backref: Backref) -> Tuple[Anchor, Optional[Block]]:
        a = self.lookup_backref(backref)
        return a, self._anchored_floats.get(a)
