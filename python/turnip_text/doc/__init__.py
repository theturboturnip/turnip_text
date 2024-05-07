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
    Document,
    Header,
    Inline,
    TurnipTextSource,
    parse_file,
)
from turnip_text.build_system import BuildSystem
from turnip_text.doc.anchors import Anchor, Backref

__all__ = [
    "Document",
    "parse_pass",
    "mutate_pass",
    "DocPlugin",
    "DocEnv",
    "FmtEnv",
    "anchors",
    "in_doc",
    "pure_fmt",
]

TDocPlugin = TypeVar("TDocPlugin", bound="DocPlugin")
P = ParamSpec("P")
T = TypeVar("T")


class DocSetup:
    build_sys: BuildSystem
    doc_project_relative_path: str
    plugins: Sequence["DocPlugin"]
    fmt: "FmtEnv"
    doc_env: "DocEnv"

    def __init__(
        self,
        build_sys: BuildSystem,
        doc_project_relative_path: str,
        plugins: Sequence["DocPlugin"],
    ) -> None:
        self.build_sys = build_sys
        self.doc_project_relative_path = doc_project_relative_path
        self.plugins = plugins
        self.fmt, self.doc_env = DocPlugin._make_contexts(build_sys, plugins)

    @property
    def anchors(self) -> "DocAnchors":
        return self.doc_env.anchors

    def parse(self) -> Document:
        src = self.build_sys.resolve_turnip_text_source(self.doc_project_relative_path)
        return parse_file(src, self.doc_env.__dict__)

    def freeze(self) -> None:
        self.doc_env._frozen = True


class DocMutator(Protocol):
    """Both DocPlugins and RenderPlugins have the option to mutate the state of the document post-parse.

    They inherit this interface."""

    def _doc_nodes(self) -> Sequence[Type[Union[Block, Inline, Header]]]:
        """
        Tell the Document what nodes this plugin exports
        """
        return []

    def _mutate_document(
        self, doc_env: "DocEnv", fmt: "FmtEnv", toplevel: Document
    ) -> Document:
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
    # Initialized when the plugin is included into the MutableState,
    # which is after the plugin is constructed.
    # Should always be non-None when the plugin's emitted functions are called.
    # Can't be handled by e.g. a metaclass, because that would require the doc_env and fmt to
    # effectively be global state.
    __doc_env: "DocEnv" = None  # type: ignore
    __fmt: "FmtEnv" = None  # type: ignore

    def __init_ctx(self, fmt: "FmtEnv", doc_env: "DocEnv") -> None:
        assert self.__doc_env is None and self.__fmt is None
        self.__doc_env = doc_env
        self.__fmt = fmt

    @property
    def _doc_env(self) -> "DocEnv":
        """Retrieve the doc_env execution environment, if the document hasn't been frozen.
        raises RuntimeError if the document is frozen."""
        if self.__doc_env._frozen:
            raise RuntimeError(
                "Can't run an in_env function, or retrieve doc_env, when the doc is frozen!"
            )
        return self.__doc_env

    @property
    def _fmt(self) -> "FmtEnv":
        """Retrieve the fmt 'format' environment with only @pure_fmt annotated functions."""
        return self.__fmt

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

        By default, finds all public variables, member functions, and static functions.
        Ignores any fields that begin with _.
        Ignores properties.
        Based on https://github.com/python/cpython/blob/a0773b89dfe5cd2190d539905dd89e7f6455668e/Lib/inspect.py#L562C5-L562C5.

        May be overridden."""

        interface = {}
        names = dir(self)
        # Ignore "DynamicClassAttributes" for now, `self` isn't a class so doesn't have them
        for key in names:
            if key.startswith("_"):
                continue
            value = inspect.getattr_static(self, key)
            if (
                isinstance(value, property)
                or inspect.ismethoddescriptor(value)
                or inspect.isdatadescriptor(value)
            ):
                # We can't pass properties through - these are used via a Dict which is used as globals() and __get__ wouldn't be called.
                # If you want to access mutable state, do it through a function.
                print(
                    f"Property {key} on {self} is not going to be used in the plugin interface, because we can't enforce calling __get__(). Expose getter and setter functions instead."
                )
            else:
                # Use getattr to do everything else as normal e.g. bind methods to the object etc.
                interface[key] = getattr(self, key)

        return interface

    @staticmethod
    def _make_contexts(
        build_sys: BuildSystem,
        plugins: Sequence["DocPlugin"],
    ) -> Tuple["FmtEnv", "DocEnv"]:
        anchors = DocAnchors()
        fmt = FmtEnv()
        doc_env = DocEnv(build_sys, fmt, anchors)

        def register_plugin(plugin: "DocPlugin") -> None:
            i = plugin._interface()

            for key, value in i.items():
                # The pure_fmt context only includes:
                # - functions and methods that have been explicitly marked pure_fmt.
                # - class variables which aren't data descriptors (they don't have access to the plugin self and don't have access to __doc_env.)

                if key in RESERVED_DOC_PLUGIN_EXPORTS:
                    print(f"Warning: ignoring reserved field {key} of plugin {plugin}")
                    continue

                is_pure_fmt = False
                if inspect.ismethod(value):
                    is_pure_fmt = getattr(value.__func__, "_pure_fmt", False)
                elif hasattr(value, "__call__"):
                    # Callables or data descriptors need to be explicitly marked as pure_fmt
                    is_pure_fmt = getattr(value, "_pure_fmt", False)
                else:
                    # It's not a bound method, it's not callable or gettable, it's probably a plain variable.
                    # It can't get access to __doc_env, so expose it.
                    is_pure_fmt = True

                # No matter what, the function gets added to the in_doc context (even if they're pure_fmt!)
                # If you're in the middle of building the document, you're allowed to call pure_fmt functions too.
                doc_env.__dict__[key] = value
                # pure_fmt functions get added to the pure_fmt context
                if is_pure_fmt:
                    fmt.__dict__[key] = value

            plugin.__init_ctx(fmt, doc_env)

        register_plugin(anchors)
        for plugin in plugins:
            register_plugin(plugin)

        return fmt, doc_env


def in_doc(
    f: Callable[Concatenate[TDocPlugin, "DocEnv", P], T]
) -> Callable[Concatenate[TDocPlugin, P], T]:
    """
    An annotation for plugin bound methods which access the __doc_env object i.e. other in_doc (and pure_fmt) functions and variables.
    This is the only way to access the doc_env, and thus theoretically the only way to mutate doc_env.
    Unfortunately, we can't protect a plugin from modifying its private state in a so-annotated "pure_fmt" function.
    """

    def wrapper(plugin: TDocPlugin, /, *args: Any, **kwargs: Any) -> T:
        return f(plugin, plugin._doc_env, *args, **kwargs)

    wrapper._in_doc = True  # type: ignore
    wrapper.__doc__ = f.__doc__
    wrapper.__name__ = f"in_doc wrapper of function {f.__name__}()"

    return wrapper


def pure_fmt(
    f: Callable[Concatenate[TDocPlugin, "FmtEnv", P], T]
) -> Callable[Concatenate[TDocPlugin, P], T]:
    """
        An annotation for plugin bound methods which access the __fmt object i.e. other pure_fmt functions.
        This is the only way to access fmt.
        Unfortunately, we can't protect a plugin from modifying its private state in a so-annotated "pure_fmt" function.

    ```
        class SomePlugin(Plugin):
            @pure_fmt
            def formatting_func(self, fmt, other_arg):
                return (fmt.bold @ other_arg)
    ```

        some_plugin.formatting_func(other_arg) # returns (fmt.bold @ other_arg), don't need to pass in fmt!
    """

    def wrapper(plugin: TDocPlugin, /, *args: Any, **kwargs: Any) -> T:
        return f(plugin, plugin._fmt, *args, **kwargs)

    wrapper._pure_fmt = True  # type: ignore
    wrapper.__doc__ = f.__doc__
    wrapper.__name__ = f"pure_fmt wrapper of function {f.__name__}()"

    return wrapper


RESERVED_DOC_PLUGIN_EXPORTS = [
    "build_sys",
    "doc",
    "fmt",
    "anchors",
    "backref",
]


class FmtEnv:
    def __getattr__(self, name: str) -> Any:
        # The FmtEnv has various things that we don't know at type-time.
        # We want to be able to use those things from Python code.
        # __getattr__ is called when an attribute access is attempted and the "normal routes" fail.
        # Defining __getattr__ tells the type checker "hey, this object has various dynamic attributes"
        # and the type checker will then allow Python code to use arbitrary attributes on this object.
        # We still need to raise AttributeError here, because we don't actually define any attributes this way.
        raise AttributeError(name=name, obj=self)


class DocEnv:
    _frozen: bool = (
        False  # Set to True when rendering the document, which disables functions annotated with @in_doc.
    )

    # These are reserved fields, so plugins can't export them.
    # Evaluated code can call directly out to doc.blah or fmt.blah.
    build_sys: BuildSystem
    doc: "DocEnv"
    fmt: "FmtEnv"
    # TODO don't special-case DocAnchors
    anchors: "DocAnchors"
    # TODO move this into DocAnchors plugin
    # This can be used by all document code to create backrefs, optionally with custom labels.
    backref = Backref

    def __init__(
        self, build_sys: BuildSystem, fmt: "FmtEnv", anchors: "DocAnchors"
    ) -> None:
        self.build_sys = build_sys
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

    This is a DocPlugin so that it can use the @in_doc annotation to avoid creating new anchors after the document is frozen.
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

    @in_doc
    def register_new_anchor(
        self, doc_env: DocEnv, kind: str, id: Optional[str]
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
