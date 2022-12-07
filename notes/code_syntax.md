# Code-in-text

## Syntax

no backslashes - confuses with character escaping

rust macro syntax is nice

`[x = blah()]`

TODO port notes over from my tablet

## Language Choice

Language must be 
- Embeddable
- Intuitive
- Easy to integrate with the Rust parser
- Sandboxable(ish)?
  - If user tries to reassign a pre-existing value, prob don't let that happen

### Python?

Initial choice

PyO3 can embed a python interpreter
- but seemingly cannot reset it?
  - [with_embedded_python_interpreter](https://docs.rs/pyo3/latest/pyo3/fn.with_embedded_python_interpreter.html)
  - "many Python modules implemented in C do not support multiple Python interpreters in a single process, it is not safe to call this function more than once."

batteries included library

but hooking Rust into it seems difficult. like, really difficult.

Ways to embed python
- [PyO3](https://github.com/pyo3/pyo3)
  - manual ffi [pyo3-ffi](https://crates.io/crates/pyo3-ffi/0.16.5)
  - build a shared lib as a "native python module" [maturin](https://github.com/PyO3/maturin) NOT WHAT WE WANT
  - TODO test if the macros for adding python modules etc works when embedding, not just with maturin
- entirely manual [https://docs.python.org/3/extending/embedding.html]
- [RustPython](https://rustpython.github.io/) (reimplemetation in python)
  - seems unstable and not ready
- [pyembed](https://docs.rs/pyembed/latest/pyembed/index.html)
  - lots of options for the allocator, not so much for adding new functions

If we use python, how does that interface with virtualenvs?
Is PyO3 essentially doing a dylink with the nearest "python 3.X DLL"? That sounds like it would *just work* in a venv

### Lua?

Has mature embedder library for Rust [rlua](https://github.com/amethyst/rlua)

[Locking global variables](http://lua-users.org/lists/lua-l/2003-08/msg00081.html)

confusion with lualatex

lua can return functions, so this is possible
```lua
function bold()
    return function (x)
        return format("bold", x)
    end
end
```

## Lifecycle

LaTeX infamously requires multiple runs because back-references (e.g. table of contents) can't be constructed at the first reference.
e.g. when you create a table of contents, one pass through the file is required to create the `.toc`, then another run detects that file and includes the relevant references.
If we want this to create a markdown version, we'd have to embed the ToC ourselves, and ideally we don't do multiple runs.
TODO figure out how this looks from a scripting perspective.