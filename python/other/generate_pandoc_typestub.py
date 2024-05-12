import argparse
import re
from typing import *

import pandoc.types as pan  # type: ignore

"""Use the string representations of base Pandoc types to generate typestubs"""


def smart_arg_split(inner: str) -> List[str]:
    items = []
    depth = 0
    item_in_progress = ""
    for char in inner:
        match char:
            case " ":
                if depth == 0:
                    continue
                else:
                    item_in_progress += char
            case ",":
                if depth == 0:
                    assert item_in_progress != ""
                    items.append(item_in_progress)
                    item_in_progress = ""
                else:
                    item_in_progress += char
            case "(":
                item_in_progress += char
                depth += 1
            case "[":
                item_in_progress += char
                depth += 1
            case "]":
                item_in_progress += char
                depth -= 1
            case ")":
                item_in_progress += char
                depth -= 1
            case other:
                item_in_progress += char
    assert depth == 0
    if item_in_progress:
        items.append(item_in_progress)
    return items


def parse_pan_type_str(s: str, on_fwd_ref: Callable[[str], None]) -> str:
    s = s.strip()
    if s.startswith("{"):
        # Dict!
        # Remove end characters
        inner = s[1:-1]
        key, value = inner.split(":")
        return f"Dict[{parse_pan_type_str(key, on_fwd_ref)}, {parse_pan_type_str(value, on_fwd_ref)}]"
    elif s.startswith("("):
        # Tuple!
        # Remove end characters
        inner = s[1:-1]
        # Split on commas, convert each elem into a type, make a generic Tuple
        inner_types = [
            parse_pan_type_str(sub_elem, on_fwd_ref)
            for sub_elem in smart_arg_split(inner)
            if sub_elem.strip()
        ]
        return f"Tuple[{', '.join(inner_types)}]"
    elif s.startswith("["):
        # List!
        # Remove end characters
        inner = s[1:-1]
        # Parse inner
        inner_type = parse_pan_type_str(inner, on_fwd_ref)
        return f"List[{inner_type}]"
    elif s.endswith("orNone") or s.endswith("or None"):
        # HACK: smart_arg_split doesn't handle 'or None' gracefully, and returns e.g. "ShortCaptionorNone"
        new_s = re.sub(r"or\s?None$", "", s)
        return f"Optional[{parse_pan_type_str(new_s, on_fwd_ref)}]"
    else:
        # Assume it's a more complicated type, forward refernce
        on_fwd_ref(s)
        return f'"{s}"'


PAN_CONSTRUCTOR_REGEX = re.compile(r"(\w+)\((.*)\)$")


def constructor_typedef(
    constructor_str: str, on_sub_type: Callable[[str], None], superclass: str = ""
) -> Tuple[str, str]:
    # Assume this isn't of a union with a subclass
    match = PAN_CONSTRUCTOR_REGEX.match(constructor_str.strip())
    if not match:
        raise RuntimeError(f"data_type {constructor_str} didn't match regex")
    subclass_type_name = match.group(1)
    subclass_type_args = [
        arg for arg in smart_arg_split(match.group(2).strip()) if arg.strip()
    ]
    arg_type_list = (
        [parse_pan_type_str(arg, on_sub_type) for arg in subclass_type_args]
        if subclass_type_args
        else []
    )
    init_arg_list = [f"arg{i}: {arg_type}" for i, arg_type in enumerate(arg_type_list)]

    class_def = f"class {subclass_type_name}"
    if superclass:
        class_def += f"({superclass})"
    class_def += (
        f":\n\tdef __init__(self, {', '.join(init_arg_list)}) -> None:\n\t\t...\n"
    )

    for i, arg_type in enumerate(arg_type_list):
        if len(arg_type_list) > 1:
            class_def += f"\t@overload\n"
        class_def += (
            f"\tdef __getitem__(self, index: Literal[{i}]) -> {arg_type}:\n\t\t...\n"
        )

    for i, arg_type in enumerate(arg_type_list):
        if len(arg_type_list) > 1:
            class_def += f"\t@overload\n"
        class_def += f"\tdef __setitem__(self, index: Literal[{i}], obj: {arg_type}) -> None:\n\t\t...\n"

    return subclass_type_name, class_def


def main(out_path: str) -> None:
    # A stack of types to examine
    type_queue: List[str] = ["Pandoc"]
    # A set of types we've already examined
    examined_types: Set[str] = set()
    # A set of builtin Python types some types alias to
    builtin_types: Set[Type] = {str, int, float, bool, dict, list, set}

    type_defs: Dict[str, str] = {}

    def add_type(t: str) -> None:
        type_queue.append(t)

    while type_queue:
        type_name = type_queue.pop()
        if type_name in examined_types:
            continue

        actual_type: Type = getattr(pan, type_name)

        if actual_type in builtin_types:
            type_defs[type_name] = f"{type_name} = {actual_type.__name__}"
        elif issubclass(actual_type, pan.TypeDef):
            # This is a basic typedef
            # e.g. pan.Attr -> 'Attr = (Text, [Text], [(Text, Text)])
            rhs = str(actual_type).split(" = ")[1]
            # Parse the RHS, adding new types to the list if they're included inside
            type_defs[type_name] = f"{type_name} = {parse_pan_type_str(rhs, add_type)}"
        elif issubclass(actual_type, pan.Data):
            if issubclass(actual_type, pan.Constructor):
                _, class_def = constructor_typedef(str(actual_type), add_type)
                type_defs[type_name] = class_def
                examined_types.add(type_name)
            else:
                # Abstract datatype = a union type
                # Empty class definition
                type_defs[type_name] = f"class {type_name}:\n\tpass\n"
                # Add the union elements to the search stack
                data_types = str(actual_type).split(" = ")[1].split("|")
                # str(actual_type) formatted e.g. as
                # Block = Plain([Inline])
                #       | Para([Inline])
                #       | LineBlock([[Inline]])
                #       | CodeBlock(Attr, Text)
                # => take RHS to remove Block = , split on |, then strip and take the first part before ( (if present)
                for data_type in data_types:
                    subclass_type_name, class_def = constructor_typedef(
                        data_type, add_type, superclass=type_name
                    )
                    type_defs[subclass_type_name] = class_def
                    examined_types.add(subclass_type_name)
        else:
            raise RuntimeError(
                f"Don't know how to handle type {type_name} = {actual_type}"
            )

        examined_types.add(type_name)

    with open(out_path, "w", encoding="utf-8") as f:
        f.write("# Autogenerated by generate_pandoc_typestub.py\n\n")
        f.write("from typing import Dict, List, Tuple, Optional, Literal\n")
        f.write("from typing_extensions import overload\n\n")
        for type_name, type_def in type_defs.items():
            f.write(type_def)
            f.write("\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("out_path")
    args = parser.parse_args()
    main(str(args.out_path))
