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
    Protocol,
    Self,
    Sequence,
    Set,
    Tuple,
    Type,
    TypeVar,
    Union,
    overload,
)

T = TypeVar("T")
P = ParamSpec("P")
TReturn = TypeVar("TReturn")


# TODO make this put registered-type last
class DynDispatch(Generic[P, TReturn]):
    """This class allows you to register "handlers" for types and retrieve them for an object of a registered type.

    If the exact type of the object has a handler, that will be retrieved.
    Otherwise inheritance is resolved on a first-come-first-served basis - the first registered type that the object is an instance of is chosen.

    You can specify a paramspec for extra arguments to the handler, including the object, and the return type must be consistent for all functions.
    For example, `DynDispatch[[X, Y], R]` will map functions
    - `T1 -> Callable[[T1, X, Y], R]`
    - `T2 -> Callable[[T2, X, Y], R]`"""

    # We know that we only assign _table[t] = f if f takes t, and that when we pull it
    # out we will always call f with something of type t.
    # mypy doesn't know that, so we say _table stores functions taking Any
    # and sweep the difference under the rug
    _table: Dict[Type[Any], Callable[Concatenate[Any, P], TReturn]]

    def __init__(self) -> None:
        super().__init__()
        self._table = {}

    def register_handler(
        self,
        t: Type[T],
        f: Callable[Concatenate[T, P], TReturn],
    ) -> None:
        if t in self._table:
            raise RuntimeError(f"Conflict: registered two handlers for {t}")
        self._table[t] = f

    def get_handler(self, obj: T) -> Callable[Concatenate[T, P], TReturn] | None:
        # type-ignores are used here because mypy can't tell we'll always
        # return a Callable[[T, P], TReturn] for any obj: T.
        # This is because we only ever store T: Callable[[T, P], TReturn] in _table.
        f = self._table.get(type(obj))
        if f is None:
            for t, f in self._table.items():
                if isinstance(obj, t):
                    return f
            return None
        else:
            return f

    def keys(self) -> Iterable[Type[Any]]:
        return self._table.keys()
