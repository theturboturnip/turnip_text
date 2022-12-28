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

#### Embedding python is hard
Tried using PyO3, which as expected does not support using the macros e.g. `#[pymodule]` for attaching modules before the interpreter starts running.

Looked at using `pyembed` to initialize the interpreter - this requires a bunch of CPython FFI, and unfortunately PyO3 doesn't expose the magic it does to make functions FFI-able.
For example, the `#[pyfunction]` macro will create a `PyMethodDef` and a function harness to convert the raw python arguments (e.g. `self: *mut PyObject, args_tuple: *mut PyObject` for `METH_VARARGS`) into Rust-y equivalents, but this is created as `::pyo3::impl_::pyfunction::PyMethodDef`.
This can only be converted to `pyo3::ffi::PyMethodDef` with a `pub(crate)` function, _so I can't use it_ when I call `PyModule_Create`.
That is mildly infuriating.

For now, a holdover solution would be to manually create the PyModule and force it into the globals dict.
```rust
let module = PyModule::new(py, "turnip_text")?;
module.add_function(wrap_pyfunction!(experiment, module)?)?;
let globals = [("turnip_text", module)].into_py_dict(py);
```
This means you can't do `import turnip_text`, but that isn't the end of the world?

Actual solution - PyO3 generates a `PyInit` function for modules created with `#[pymodule]` - just use that lol

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

Two passes: first pass creates a bunch of python objects and `eval`-s code[^1]
e.g. `lorem ipsum [Fudge()] sit dolor` creates `["lorem ipsum ", Fudge(), " sit dolor"]`.
Once that list is created, and any code has run that may impact what the python objects describe, including the typesetting/formatting code, pass through the list in order calling `render()` on those python objects.
TODO need to decide what `render()` actually creates. Does it write out raw Markdown/LaTeX?

[^1]: See [https://stackoverflow.com/questions/2220699/whats-the-difference-between-eval-exec-and-compile], this means the code in square brackets needs to be a single expression, not a statement/set of statements executed for their side-effects

## Eval-brackets
I want the embedded scripting square-bracket syntax to work with many situations
1. Expressions that result in plain text
  - `[5+7]` emits the text "12"
2. Modifying some text inside a sub-scope
  - `[emph]{emphasised text}` emits text with some backend-dependent wrapping e.g. `__emphasised text__` for markdown
3. Calling impure functions for their side effects
  - `[add_float(...)]{caption}` shouldn't emit any text immediately
4. Both 2 and 3 at once - e.g. "Titled Labels"
  - `[section]{The First Section}` should be valid?
  - `[section("sec:first"){The First Section}]` should also be valid
    - This mutates global state (a list of labels?) by adding "sec:first"
  - THOUGHT: in this case, `[section()]` is probably fine syntax
5. DEBATE: Are there compelling use cases for assigning to variables inside eval-brackets?
  - for now, no
  - What would `[x = 5]` emit? Python REPL doesn't emit any text
6. Affecting eval-brackets within a sub-scope
  - e.g. inside `[math]` in Markdown/MathJax, most formatting macros should probably be disabled

What rules should Rust use to handle these consistently?
Current draft:
- `eval` the thing inside the square brackets
  - This evaluates side-effects in impure functions => handles (3)
- if the result subclasses `ScopeOwner`...
  - if the eval-brackets are followed by an inline scope
    - parse that scope block into a `Sentence` TODO how to handle raw inline scopes
    - `result = scope_owner.create_inline(sentence)` (2)
  - elif the eval-brackets are followed by a block scope and 
    - parse that scope block into either a `RawTextBlock` or a set of `Paragraph`s (raw vs. not raw)
    - `result = scope_owner.create_explicit(content)`
  - elif the result subclasses `BlockNode` or `InlineNode`
    - jump out of `if ScopeOwner`
  - else
    - it subclasses `ScopeOwner` but not `{Block,Inline}Node` => it must be attached to a scope, but it doesn't have an appropriate scope
    - throw errors
  - if `result` (after being modified by previous ifs) implements `ImplicitBlockScopeOwner` (TODO should that be a separate type from ScopeOwner)
    - start an implicit block & break out
    - TODO how could an implicit block restrict what appears within it? should it be able to? 
- if the result (after being modified by previous ifs) subclasses `BlockNode` or `InlineNode`
  - insert it directly into the current parent scope appropriately
- else
  - insert `UnescapedText(str(result))` (1)


```python
def handle(code, following_scope=None)
  result = eval(code)
  if isinstance(ScopeOwner, result):
    if isinstance(InlineScope, following_scope) and result.can_inline():
      result = result.create_inline(following_scope)
    elif isinstance(BlockScope, following_scope) and result.can_explicit_block():
      result = result.create_explicit_block(following_scope)
    else:
      raise RuntimeError(f"ScopeOwner {result} not followed by scope")

  if isinstance((BlockNode, InlineNode), result):
    emit(result)
    if isinstance(ImplicitBlockScopeOwner, result):
      start_implicit_block(result)
  else:
    emit(UnescapedText(str(result)))
```
  


TODO we still need to figure out how `no [code] inside [math]` works


## Stages of execution for complex backends
1. Python Execution
  - results in a sequence of things that are either text-to-escape, or backend-specific-stuff-wrapping-text-to-escape


Talked about this with dad
- a tree structure is very nice
  - until now I was envisioning that Rust, after processing all text, would have a list of root objects (which could be Python objects that have within them children and a tree structure)
  - Would we want rust to have a tree structure too?
- Was previously struggling with how to restrict certain things from happening within scopes
  - i.e. inside `[math]` you can't have `[code]`, or inside `[list] [item]` you can't have `[section]`
  - enforce this by classes?
    - i.e. "`[code]` format is inline, math mode disables other inline formatting" or "`[section]` is only allowed at the top level of `[chapter]`"
