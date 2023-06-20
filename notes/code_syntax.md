# Code-in-text

## TODO
- Allow scopeless eval-brackets at the starts of paragraphs to return things fitting Block and inject them directly into the document. (This behaviour already works for Inline)
  - Don't allow subsequent text to start a new paragraph - e.g. `[block_expr] some extra text` shouldn't be valid
- Use `eval(compile(code, context, "exec"))` to support assignment to the globals/locals dict in eval-brackets https://stackoverflow.com/a/29456463
- Better raw-scope-owning behaviour: maybe a raw-scope-owner is just something with a function that takes str? and can return either Block or Inline, and the parser adapts to this? (See above)

## Current syntax

Code exists in two contexts: block-level and inline-level.
If an eval-bracket `[]` is opened outside of a paragraph, the contents are evaluated and two checks are performed:
  - if the result fits the `BlockScopeOwner` typeclass, it is expected that the eval brackets will be followed by a block scope (opened with squiggly brace + newline). The eval-result will be called with the contents of that scope (TODO when the block scope is closed), and *that* result will be `render()`-ed in the finalization phase.
  
  ```
  [test_block_owner]{
    Paragraph one!

    Paragraph two!
  }

  =>
  
  test_block_owner.render_block([Paragraph("Paragraph one!"), Paragraph("Paragraph two!")])
  ```
  - if the result fits the `InlineScopeOwner` typeclass, it is treated as the start of a new paragraph. it is expected the eval brackets will be followed by an inline scope (opened with squiggly brace w/out newline). The eval-result will be called with the contents of that scope when the inline scope is closed, and *that* result will be `render()`-ed in the finalization phase.
  
  ```
  [test_inline]{inline text to annotate}

  =>

  test_inline.render_inline(["inline text to annotate"])
  ```
  - otherwise the result is treated as the start of a new paragraph. The python object is placed directly into the sentence, and will be stringified in the finalization phase. (TODO this isn't the case anymore, now it's rejected because it isn't Inline. Need type coersion for this)

  ```
  [5+7]

  =>

  str(12)
  ```
When an eval-bracket is opened inside a paragraph, it can be either an `InlineScopeOwner` or a plain object. A block scope may not be opened mid-paragraph.

Code is evaluated in THREE phases.
1. While the document tree is being built, the code inside eval-brackets is evaluated immediately i.e. in the order they are encountered in the document.
   1. if the eval-bracket-result is a scope owner, it is invoked with the contents of its scope once the scope is closed.
2. Once the document tree is finished, Python is invoked on the completed tree to render the document out, which may involve invoking methods on the eval-bracket-results.

### Original idea

No backslashes - confuses with character escaping

Rust macro syntax is nice

`[textbf(text to boldface)]`?

Problem with above - would need to implicitly convert "text to boldface" to a string literal, the heuristics for that seem hairy and limiting

Could try
1. ```[textbf { text to boldface }]``` - use a separate syntax for "env" types
   1. But it has ambiguity with python dicts
   2. How could you supply optional args?
2. ```[textbf(option) { text to boldface }]``` - add optional args
   1. Still has ambiguity with python
3. ```[textbf(option)]{ text to boldface }``` - move squiggle braces outside of the code brackets
   1. Ambiguity resolved, because the python code stops at the square bracket

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
- entirely manual [https://docs.python.org/3/extending/embedding.html]
- [RustPython](https://rustpython.github.io/) (reimplemetation of python in rust)
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

I don't know Lua as well as I do Python, and Python has better syntax for some of the stuff I want to do (e.g. classes?)

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
2. Inline markup
    - `[emph]{emphasised text}` emits text with some backend-dependent wrapping e.g. `__emphasised text__` for markdown
3. Calling impure functions for their side effects
    - `[add_float(...)]` shouldn't emit anything text immediately
4. Document structure
    - `[section(r"The First Section")]`
    - `[section(r"The First Section", label="sec:first")]`
      - This mutates global state (a list of labels?) by adding "sec:first"
5. DEBATE: Are there compelling use cases for assigning to variables inside eval-brackets?
    - for now, no
       - YES FOR SIMPLE MACROS (hindsight)
    - What would `[x = 5]` emit? Python REPL doesn't emit any text
6. Affecting eval-brackets within a sub-scope
    - e.g. inside `[math]` in Markdown/MathJax, most formatting macros should probably be disabled
    - for now, just use raw text mode
    - other example: inside `[enumerate]`, only `[item]`s can be allowed at the top level?
      - Don't try to solve this here!! Python has a type system, don't be afraid to embed text inside Python code and eval-brackets

We can be strict about block vs inline scopes.
We need block scopes for custom block types a la AsciiDoctor.
We need inline scopes for easily-readable inline formatting e.g. `[emph]{stuff}` is nicer than `[emph(r"stuff")]`.
Strict way to delimit them: the only types of scopes allowed are
- Block scopes, potentially begun by a code token at the start of a line and directly followed by a newline, which may contain any tokens (including paragraph breaks) until ending
  - `^` `CODE?` `SCOPE_BEGIN` `NEWLINE` `[^SCOPE_END]*` `SCOPE_END`
- Inline scopes, potentially begun by a code block and not directly followed by a newline, which may not contain paragraph breaks but may contain other tokens until ending
  - `CODE?` `SCOPE_BEGIN` `([^SCOPE_END NEWLINE] [^SCOPE_END PARA_BREAK]*)` `SCOPE_END`



Talked about this with dad
- a tree structure is very nice
  - until now I was envisioning that Rust, after processing all text, would have a list of root objects (which could be Python objects that have within them children and a tree structure)
  - Would we want rust to have a tree structure too?
- Was previously struggling with how to restrict certain things from happening within scopes
  - i.e. inside `[math]` you can't have `[code]`, or inside `[list] [item]` you can't have `[section]`
  - enforce this by classes?
    - i.e. "`[code]` format is inline, math mode disables other inline formatting" or "`[section]` is only allowed at the top level of `[chapter]`"
