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
    print(inner, items)
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
        to_examine = type_queue.pop()
        if to_examine in examined_types:
            continue

        actual_type: Type = getattr(pan, to_examine)

        if actual_type in builtin_types:
            type_defs[to_examine] = f"{to_examine} = {actual_type.__name__}"
        elif issubclass(actual_type, pan.TypeDef):
            # This is a basic typedef
            # e.g. pan.Attr -> 'Attr = (Text, [Text], [(Text, Text)])
            rhs = str(actual_type).split(" = ")[1]
            # Parse the RHS, adding new types to the list if they're included inside
            type_defs[to_examine] = (
                f"{to_examine} = {parse_pan_type_str(rhs, add_type)}"
            )
        elif issubclass(actual_type, pan.Data):
            if issubclass(actual_type, pan.Constructor):
                data_type = str(actual_type)
                # Assume this isn't of a union with a subclass
                match = PAN_CONSTRUCTOR_REGEX.match(data_type.strip())
                if not match:
                    raise RuntimeError(f"data_type {data_type} didn't match regex")
                subclass_type_name = match.group(1)
                subclass_type_args = [
                    arg
                    for arg in smart_arg_split(match.group(2).strip())
                    if arg.strip()
                ]
                arg_list = (
                    [
                        f"arg{i}: {parse_pan_type_str(arg, add_type)}"
                        for (i, arg) in enumerate(subclass_type_args)
                    ]
                    if subclass_type_args
                    else []
                )
                type_defs[subclass_type_name] = (
                    f"class {subclass_type_name}:\n\tdef __init__(self, {', '.join(arg_list)}):\n\t\t...\n"
                )
                examined_types.add(subclass_type_name)
            else:
                # Abstract datatype = a union type
                # Empty class definition
                type_defs[to_examine] = f"class {to_examine}:\n\tpass\n"
                # Add the union elements to the search stack
                data_types = str(actual_type).split(" = ")[1].split("|")
                # str(actual_type) formatted e.g. as
                # Block = Plain([Inline])
                #       | Para([Inline])
                #       | LineBlock([[Inline]])
                #       | CodeBlock(Attr, Text)
                # => take RHS to remove Block = , split on |, then strip and take the first part before ( (if present)
                for data_type in data_types:
                    match = PAN_CONSTRUCTOR_REGEX.match(data_type.strip())
                    if not match:
                        raise RuntimeError(f"data_type {data_type} didn't match regex")
                    subclass_type_name = match.group(1)
                    subclass_type_args = [
                        arg
                        for arg in smart_arg_split(match.group(2).strip())
                        if arg.strip()
                    ]
                    print(data_type, subclass_type_name, subclass_type_args)
                    arg_list = (
                        [
                            f"arg{i}: {parse_pan_type_str(arg, add_type)}"
                            for (i, arg) in enumerate(subclass_type_args)
                        ]
                        if subclass_type_args
                        else []
                    )
                    type_defs[subclass_type_name] = (
                        f"class {subclass_type_name}({to_examine}):\n\tdef __init__(self, {', '.join(arg_list)}):\n\t\t...\n"
                    )
                    examined_types.add(subclass_type_name)
        else:
            raise RuntimeError(
                f"Don't know how to handle type {to_examine} = {actual_type}"
            )

        examined_types.add(to_examine)

    with open(out_path, "w", encoding="utf-8") as f:
        f.write("# Autogenerated by generate_pandoc_typestub.py\n\n")
        f.write("from typing import Dict, List, Tuple, Optional\n\n")
        for type_name, type_def in type_defs.items():
            f.write(type_def)
            f.write("\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("out_path")
    args = parser.parse_args()
    main(str(args.out_path))
