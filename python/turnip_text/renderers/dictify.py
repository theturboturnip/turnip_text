from typing import Any, Dict, List, Mapping


class dictify_pure_property(property):
    """Equivalent to `property`, but acts as a purity marker.
    This should only be used if invoking the property itself DOESN'T MUTATE STATE.
    This means calling it once is equivalent to calling it many times, which is a useful property when dictifying."""
    pass

def dictify(r: Any) -> Dict[str, Any]:
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