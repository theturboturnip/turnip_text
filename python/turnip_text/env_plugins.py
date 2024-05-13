import inspect
from typing import (
    Any,
    Callable,
    Concatenate,
    Dict,
    ParamSpec,
    Sequence,
    Tuple,
    Type,
    TypeVar,
    Union,
)

from turnip_text import Block, Document, Header, Inline
from turnip_text.build_system import BuildSystem
from turnip_text.doc.anchors import Backref

T = TypeVar("T")
TBlockOrInline = TypeVar("TBlockOrInline", bound=Union[Block, Inline])
THeader = TypeVar("THeader", bound=Header)
TVisitable = TypeVar("TVisitable", bound=Union[Block, Inline, Header])
TVisitorOutcome = TypeVar("TVisitorOutcome")

TEnvPlugin = TypeVar("TEnvPlugin", bound="EnvPlugin")
P = ParamSpec("P")


class EnvPlugin:
    """
    The base class for all plugins that provide functions for
    the turnip_text document/formatting *environments*, hence the name.
    """

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

    def _doc_nodes(self) -> Sequence[Type[Union[Block, Inline, Header]]]:
        """
        Tell the Document what nodes this plugin exports
        """
        return []

    def _mutate_document(self, doc_env: "DocEnv", fmt: "FmtEnv", doc: Document) -> None:
        """
        Mutate the document as you please.
        """
        return None

    def _interface(self) -> Dict[str, Any]:
        """
        Define the interface available to the document/formatting environments,
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
        plugins: Sequence["EnvPlugin"],
    ) -> Tuple["FmtEnv", "DocEnv"]:
        """Given a set of EnvPlugins, build a FmtEnv with annotated @pure_fmt functions + plain value, and DocEnv with annotated @in_doc functions, from the contents of all plugins.
        Is a method of EnvPlugin so it can use internal methods that begin with __"""
        fmt = FmtEnv()
        doc_env = DocEnv(build_sys, fmt)

        def register_plugin(plugin: "EnvPlugin") -> None:
            i = plugin._interface()

            for key, value in i.items():
                # The pure_fmt context only includes:
                # - functions and methods that have been explicitly marked pure_fmt.
                # - class variables which aren't data descriptors (they don't have access to the plugin self and don't have access to __doc_env.)

                if key in RESERVED_ENV_PLUGIN_EXPORTS:
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

                # TODO accumulate type hints for methods here?

                # No matter what, the function gets added to the in_doc context (even if they're pure_fmt!)
                # If you're in the middle of building the document, you're allowed to call pure_fmt functions too.
                doc_env.__dict__[key] = value
                # pure_fmt functions get added to the pure_fmt context
                if is_pure_fmt:
                    fmt.__dict__[key] = value

            plugin.__init_ctx(fmt, doc_env)

        for plugin in plugins:
            register_plugin(plugin)

        return fmt, doc_env


def in_doc(
    f: Callable[Concatenate[TEnvPlugin, "DocEnv", P], T]
) -> Callable[Concatenate[TEnvPlugin, P], T]:
    """
    An annotation for plugin bound methods which access the __doc_env object i.e. other in_doc (and pure_fmt) functions and variables.
    This is the only way to access the doc_env, and thus theoretically the only way to mutate doc_env.
    Unfortunately, we can't protect a plugin from modifying its private state in a so-annotated "pure_fmt" function.
    """

    def wrapper(plugin: TEnvPlugin, /, *args: Any, **kwargs: Any) -> T:
        return f(plugin, plugin._doc_env, *args, **kwargs)

    wrapper._in_doc = True  # type: ignore
    wrapper.__doc__ = f.__doc__
    wrapper.__name__ = f"in_doc wrapper of function {f.__name__}()"

    return wrapper


def pure_fmt(
    f: Callable[Concatenate[TEnvPlugin, "FmtEnv", P], T]
) -> Callable[Concatenate[TEnvPlugin, P], T]:
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

    def wrapper(plugin: TEnvPlugin, /, *args: Any, **kwargs: Any) -> T:
        return f(plugin, plugin._fmt, *args, **kwargs)

    wrapper._pure_fmt = True  # type: ignore
    wrapper.__doc__ = f.__doc__
    wrapper.__name__ = f"pure_fmt wrapper of function {f.__name__}()"

    return wrapper


RESERVED_ENV_PLUGIN_EXPORTS = [
    "build_sys",
    "doc",
    "fmt",
    "anchors",
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

    def __init__(self, build_sys: BuildSystem, fmt: "FmtEnv") -> None:
        self.build_sys = build_sys
        self.doc = self
        self.fmt = fmt

    def __getattr__(self, name: str) -> Any:
        # The DocEnv has various things that we don't know at type-time.
        # We want to be able to use those things from Python code.
        # __getattr__ is called when an attribute access is attempted and the "normal routes" fail.
        # Defining __getattr__ tells the type checker "hey, this object has various dynamic attributes"
        # and the type checker will then allow Python code to use arbitrary attributes on this object.
        # We still need to raise AttributeError here, because we don't actually define any attributes this way.
        raise AttributeError(name=name, obj=self)
