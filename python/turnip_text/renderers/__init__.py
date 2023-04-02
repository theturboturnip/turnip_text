from os import PathLike
from typing import Any, Callable, Dict, List, Mapping, Optional, Protocol, Tuple, TypeVar, Union, runtime_checkable

from turnip_text import BlockScope, BlockScopeBuilder, Inline, InlineScopeBuilder, parse_file_native

CiteKey = str
CiteKeyWithNote = Tuple[CiteKey, str]


def parse_file(p: PathLike, r: 'Renderer') -> BlockScope:
    # TODO this seems super icky
    # The problem: we want to be able to call e.g. [footnote] inside the turniptext file.
    # But if footnote were a free function, it would mutate global state -- we don't want that.
    # A hack! Require a 'renderer object' to be passed in - this encapsulates the local state.
    # Create a new dictionary, holding all of the Renderer's public fields,
    # and use that as the locals for parse_file_local.
    
    return parse_file_native(str(p), dictify_renderer(r))

class dictify_pure_property(property):
    """Equivalent to `property`, but acts as a purity marker.
    This should only be used if invoking the property itself DOESN'T MUTATE STATE.
    This means calling it once is equivalent to calling it many times, which is a useful property when dictifying."""
    pass

def dictify_renderer(r: 'Renderer') -> Dict[str, Any]:
    """
    Given an object implementing `Renderer`, get a dict of all functions, methods, and fields it exposes publically.

    Public = does not begin with '_'. This hides internal Python methods (e.g. `__str__`), name-mangled variables (both the original var `__name_mangled` and the mangled `_ExampleClass__name_mangled`, and any variables the programmer doesn't wish to expose `_plz_dont_modify_directly`.

    These are retrieved as follows:
    1. Get `dir(r)`, to get a dictionary of the fields and descriptors it exposes.
    2. Iterate through the keys, filtering out any that begin with `_` to find the public fields.
    3. use `getattr(r, key)` to get the values of those fields and descriptors, (which could be bound methods, static methods, or plain values), putting them into a new dictionary, which is returned.
        a. Warn the user if `type(r).__dict__[key]` is an impure DATA DESCRIPTOR, e.g. a property. These are evaluated ONCE inside this function, and wouldn't be repeatedly evaluated when using the returned dict.
        That is different to the usual behaviour: if `key` is a property, reading `r.key` will call `type(r).__dict__[key].__get__(...)` every time. `returned_dict[key]` holds the value returned from calling that ONCE, and repeatedly reading it will not re-invoke the property getter.
        IF THE PROPERTY IS PURE you can avoid this warning by using @dictify_pure_property to declare it.

    This can be used as the execution environment for code inside a turnip_text file.

    [1]: Information on Python "descriptors" https://docs.python.org/3.8/howto/descriptor.html
    """

    from inspect import isdatadescriptor

    r_obj_public_fields: List[str] = [
        k
        for k in dir(r)
        if not k.startswith("_")
    ]
    
    r_type_dict: Mapping[str, Any] = type(r).__dict__
    
    # Warn about impure data descriptor fields
    for k in r_obj_public_fields:
        if isdatadescriptor(r_type_dict[k]) and not isinstance(r_type_dict[k], dictify_pure_property):
            print(f"dictify_renderer Warning: renderer {r} exposes a public 'data descriptor' (e.g. a property) "
                  f"named {k!r}. This will be evaluated exactly once, and the result will be stored in the "
                  f"returned dict, instead of evaluating the property each time the dict is accessed. "
                  f"DO NOT USE THESE IF YOU CAN AVOID IT. Use a normal field instead.")

    return {
        k: getattr(r, k)
        for k in r_obj_public_fields
    }
    


@runtime_checkable
class Renderer(Protocol):
    """
    Renderers should export a consistent set of functions for documents to rely on.

    These should only use 'pure' properties, that don't mutate state, because they won't be called multiple times - see `dictify_renderer()` function for details why.
    'Pure' properties need to be marked with `@dictify_pure_property`.

    If you want impure results (i.e. once that mutate state) without function call syntax, you can use Builders.
    e.g. the `footnote` property is pure, but returns a single builder object that mutates the internal list of footnotes whenever `.build()` is called.
    """
    
    # Inline formatting
    @dictify_pure_property
    def emph(self) -> InlineScopeBuilder:
        ...

    # Citations
    def cite(self, *keys: Union[CiteKey, CiteKeyWithNote]) -> Inline: ...
    def citeauthor(self, key: CiteKey) -> Inline: ...

    # Footnotes
    @dictify_pure_property
    def footnote(self) -> InlineScopeBuilder:
        """Define an unlabelled footnote, capturing the following inline text as the contents,
        and insert a reference to that footnote here.
        
        `[footnote]{contents}` is roughly equivalent to `[footnote_ref(<some_label>)]` and `[footnote_text(<some_label>)]{contents}`"""
        ...
    def footnote_ref(self, label: str) -> Inline:
        """Insert a reference to a footnote, which will have the contents defined by a matching call to footnote_text.
        If, at render time, footnote_text has not been called with a matching label then an error will be raised."""
        ...
    def footnote_text(self, label: str) -> BlockScopeBuilder:
        """Define the contents of a footnote with the given label. Cannot be called twice with the same label."""
        ...

    # Structure
    def section(self, name: str, label: Optional[str] = None, numbered: bool = False) -> BlockScopeBuilder: ...
    def subsection(self, name: str, label: Optional[str] = None, numbered: bool = False) -> BlockScopeBuilder: ...
    def subsubsection(self, name: str, label: Optional[str] = None, numbered: bool = False) -> BlockScopeBuilder: ...

    # Figures?
