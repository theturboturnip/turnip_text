turnip_text is an opinionated language that aims for formatting in source files to map intuitively to document elements, with a minimum of hardcoded syntax.
It allows programmability through embedded Python.
The turnip_text core parser reads in source text, evaluating embedded Python snippets as it goes, to create a Python document tree.

# Initiating a turnip_text parse

Calling `turnip_text.parse_file_native(src, globals)` starts turnip_text parsing the given source file.
- `src` must be a `TurnipTextSource` instance, not a native Python file object. To create a `TurnipTextSource` instance from a file object, you have three options:

  ```python
  from turnip_text import TurnipTextSource

  src_direct    = TurnipTextSource(name="some file", contents="turnip_text source code")
  # Automatically sets name=path, and reads the file into `contents` assuming UTF-8.
  # The file must not have nul bytes '\0'. turnip_text will detect this and raise an error.
  src_from_file = TurnipTextSource.from_path("path/to/turnip_text.ttext")
  # Automatically sets name="<string>"
  src_from_str  = TurnipTextSource.from_str("turnip_text source code")
  ```
- `globals` must be a Python `dict`. It is the context in which all code is evaluated.

# The turnip_text document model

The output of `parse_file_native()` is a `Document`.
A `Document` contains front matter `content`, and a tree of `segments: List[DocSegment]`.
`DocSegments` are segments of a document with `Header`s, and allow the creation of structured documents.
For example, a book may consist of a `Document` with frontmatter (acknowledgements, table of contents), chapters, and sections within those chapters.
Each chapter is a `DocSegment` in `document.segments`, and consists of a `header` (i.e. the chapter title with suitable formatting), further `content` (the content in the chapter before the first section), and `subsegments` (the sections within the chapter).

Each `DocSegment` has a `header` - a Python object fulfilling the requirements of `Header`.
TODO a header is a kind of block
Python code can create its own kinds of `Header` by declaring a class with a property `is_header: bool = True` and another property `weight: int` which we will discuss later.
You can also make your custom class a subclass of `Header` to add these properties automatically.
There are no predefined implementations of this class, you must make your own.

```python
# Simplified, see __init__.pyi for full declaration
class Document:
    content: BlockScope
    segments: List[DocSegment]

class DocSegment:
    header: Header
    contents: BlockScope
    subsegments: List[DocSegment]

class Header(Protocol):
    is_block: bool = True
    is_header: bool = True
    weight: int = 0
```

The `contents` of `Document`s and `DocSegment`s are both `BlockScope` objects: a Python class representing a type-checked list of `Block`s.
The most basic example of a `Block` is a `Paragraph`, but `BlockScope`s themselves are also `Block`s and can thus nest inside themselves.
Python code can create its own kinds of `Block` by declaring a class with a property `is_block: bool = True`.
You can also make your custom class a subclass of `Block` to add this property automatically.

```python
# Simplified, see __init__.pyi for full declaration
class Block(Protocol):
    is_block: bool = True

# BlockScope is like a List[Block] but append-only, and typechecked
BlockScope = List[Block]
```

`Paragraph`s are blocks of text, and are usually created by parsing turnip_text source files rather than through Python.
`Paragraph`s consist of many (at least one) `Sentence`s, and each `Sentence` consists of many (at least one) `Inline` objects.
The most basic piece of inline content is `Text` representing plain UTF-8 text, but there are also `InlineScope`s (like `BlockScope`, a list of `Inline`) and `Raw` content (created through a "raw scope", which we'll see later).
Python code can create its own kinds of `Inline` by declaring a class with a property `is_inline: bool = True`.
You can also make your custom class a subclass of `Inline` to add this property automatically.

```python
# Paragraph is like a List[Sentence] but append-only, and typechecked
Paragraph = List[Sentence]

# Sentence is like a List[Inline] but append-only, and typechecked
Sentence = List[Inline]

class Inline(Protocol):
    is_inline: bool = True

class Text(Inline):
    text: str
class Raw(Inline):
    data: str
# InlineScope is like a List[Inline] but append-only, and typechecked
InlineScope = List[Inline]
```

By this point the relations between classes should be clear:

- `Document` contains `BlockScope` and `DocSegment`s, themselves containing `BlockScope`.
- `BlockScope`s contain other `Block`s: other `BlockScope`s, custom Python-defined `Block`s, and `Paragraph`s.
- `Paragraph`s contain `Sentence`s.
- `Sentence`s contain `Inline`s: plain `Text`, `Raw` content, custom Python-defined `Inline`s, and `InlineScope`s.
- `InlineScope`s contain other `Inline`s - including themselves.

The concrete classes `Document`, `DocSegment`, `BlockScope`, `Paragraph`, `Sentence`, `InlineScope`, `Text`, and `Raw` are defined in Rust and cannot be subclassed in Python.
They provide custom `__eq__` and `__repr__` functions for equality checking and debug printing respectively, but cannot be hashed.
`Block`, `Inline`, and `Header` are _typeclasses_ e.g. patterns that custom classes can fit, and are defined in Python as Protocols that you can subclass.

# Parsing a turnip_text document

The turnip_text parser starts in *top-level block mode*.
In all modes, the hash `#` character begins a comment that extends until the end of the line.
All newlines end the comment, even those escaped with backslash `\`, just like Python.

```python
Some turnip_text # with a comment
You can add an escaped newline # the comment will end... \
and the content will continue.
```

In *block mode*, the parser builds a `BlockScope` Python object out of the things you create.
As such, it is possible to create `Block`s in this mode: `Paragraph`s, `BlockScope`s, and custom Python-defined `Block` instances.
TODO headers are included here too

`Paragraph`s can be created by simply writing text.
Each line of text is a single `Sentence`.
Whitespace at the start and end of each line is ignored.
If a sentence is particularly long, you can escape the newline unlike with comments.
You cannot define multiple sentences per line.
An empty line (which may include a comment) ends the paragraph.
Note that if you escape a newline, and then have a blank line, that blank line is included with the previous line (because of the escaped newline) and does not end the paragraph.
Leading whitespace on all lines is ignored, even after escaped newlines.
Trailing whitespace is also ignored, except any whitespace between content and an escaped newline.

```
This is a paragraph.
I can continue a sentence # with an explanatory comment, even \
by escaping the following newline.

This is the second paragraph.
# You can comment out lines inside paragraphs, and they don't end the paragraph.
I'm still in the second paragraph.

This is the third paragraph. \
     
I'm still in the third paragraph. # The blank line is an extension of the first line, and doesn't end the paragraph


    An indented paragraph doesn't pick up the preceding whitespace \
    even after escaped newlines, but the space between "whitespace" and the escaped newline will remain.
```

`BlockScope`s can be created by opening a scope with a squiggly brace at the start of a line `{` and a newline.
The scope must be closed with a matching `}` before the end of the file.
A closing scope will also close any paragraphs-in-progress - although like the opening character, it must occur at the start of the line (ignoring whitespace).

```
This is a paragraph outside a block scope.

{
    This is a paragraph inside a block scope!

    # Whitespace between the line-start and the squiggly brace doesn't matter,
    # this is still a block-scope-open
    {
        This is inside a block scope inside a block scope!
    }
}
```

Closing the block scope creates a Python `BlockScope` object and pushes it into the next level up, *emitting* it into the document.
TODO note that this is fully flattened before putting it in the document
After a `BlockScope` (or any `Block`) is emitted, no content is allowed until the next line.
It is good practice to leave a blank line between the end of a block scope and the next content, but it is not required.

```
{
    Paragraph inside a block scope
} # Nothing else is allowed here...
Content can only start here!
```

Arbitrary `Block` instances can be inserted (or *emitted* into the document) by opening square brackets `[` `]`, a.k.a. *eval-brackets*.
The contents of the square brackets (which may contain newlines, no matter where the eval-brackets appear) are captured as text and evaluated as Python with the `globals` passed into `parse_file_native`.
This means any field `{ 'some_block': SomeBlock() }` in `globals` can be accessed in eval-brackets like `[some_block]`.
If you want to include square brackets inside the evaluated Python, add the same number of minus/hyphen characters to the inside of the open/close.

```
# Assuming 'some_block' is a field in `globals`, this will evaluate it.
[some_block]

# Assuming 'BlockScope' and 'some_block' are fields in `globals`, we can evaluate them.
# If you want to use "]" in your code, add `-` to the insides of the eval brackets.
[- "The string ]" -]

# Any number of - works, as long as they're balanced.
# If you want to use "-]" in your code, add extra `-` to the eval-brackets.
[--- "The string -]" ---]

# Whitespace inside the eval-brackets is trimmed from either side,
# to allow more readable code inside.
[---    BlockScope([some_block])  ---]

# Eval-brackets don't have to be expressions, they can also be statements.
# These cannot emit things into the document, they only mutate internal state.
[ x = 5 ]
# Eval-brackets can be multi-line statements, like function definitions:
[--
# Eval-brackets can contain comments too!
def fib(n):
    if n <= 1:
        return 1
    return fib(n - 1) + fib(n - 2)
--]

# These are evaluated in-order and mutate `globals`, so later statements can use them.
[fib(x)] # evaluates to fib(5) = 8
```


Note that the final eval-bracket emits `fib(5) = 8`.
TODO tests for this
turnip_text supports limited coercion: if an eval-bracket is an expression which evaluates to a string, it is automatically wrapped in `Text`.
If it evaluates to a float or an int, those are automatically stringified and converted to `Text`.
They are converted through `__str__()` so won't have pretty formatting.
Eagle-eyed readers will have noticed that `Text` isn't a `Block`, it is an `Inline`!
Eval-brackets may evaluate to four kinds of object:

- `None`, which has no effect on the document
    - If the eval-bracket is a Python statement e.g. `x = 5` it mutates state but evaluates to `None`
- A string, float, int or instance of `Inline`
    - If the parser was in *block mode*, the coerced `Inline` is placed in a `Paragraph` and parser enters *inline mode*
- An instance of `Block`, which is only allowed in *block mode* and is emitted into the enclosing `BlockScope`
- An instance of `Header`, which is only allowed in *block mode* and creates a new `DocSegment`.
TODO rewrite the header bit

TODO restructure - introduce code as something that modifies a block scope, then note that the block scope it emits is fully flattened?

TODO emitting header, weight must fit in signed int64

TODO inline mode

TODO hyphen and en/emdash expansion rules

TODO scopes

# Syntax in detail

## Code

Code can be opened anywhere in top-level, block, and inline mode.
Code is opened with a square bracket open `[` followed by N minus characters `-`, where N may be zero.
Code ends when N minus characters `-`, followed by a square bracket close `]`, are encountered.
The minuses can be used to disambiguate between a `]` for Python (e.g. closing a list) and the end of the code.
Text between the open and close (ignoring the minuses, but including newlines and *all whitespace and indentation*) is then captured and evaluated as Python code.
All newlines are converted to `\n` before evaluating.
The open and close tokens are colloquially referred to as "eval-brackets".

For flexibility, the code can be evaluated in one of three ways:
- First, the code is stripped of whitespace on both ends and compiled as a single Python expression.
    - Expressions e.g. `[1+2]`, `[x]` resolve to a value.
    - If this succeeds, the expression is evaluated with the globals passed to `parse_file_native`.
- If that throws a SyntaxError, the code is stripped of whitespace on both ends and compiled as a sequence of Python statements.
    - Statements e.g. `[x = 5]` mutate state but do not resolve to a value.
    - This allows multiline statements, function declarations etc. to be executed inside eval-brackets.
    - If this succeeds, the statements are executed with the globals passed to `parse_file_native`, and `None` is evaluated.
- If that throws an IndentationError, the code is *not* stripped of whitespace and instead surrounded in an `if True:\n` block before being compiled as a sequence of Python statements.
    - This is intended to allow consistently-indented Python code inside eval-brackets.
        - It may allow inconsistently-indented Python code inside eval-brackets, such as code which begins indented and on later lines is unindented. You shouldn't do this, and it may break with later versions.
    - If this succeeds, the statements are executed with the globals passed to `parse_file_native`, and `None` is evaluated.
    - If this fails, the compile error is thrown into the enclosing Python context TODO with the source code attached.

Once the object has been evaluated, there are two possibilities:
- If the eval-bracket is directly followed by a scope-open, the scope is processed, evaluated into a Python object, and a method is called on the eval-bracket object with the scope-object.
The return value of that function is emitted into the document.
- Otherwise, the eval-bracket object is emitted into the document.

### A note on the choice of disambiguation character

Eval-brackets are effectively a form of raw string that eventually get evaluated as Python, and need a disambiguation character to tell the difference between a string-end token inside Python (in our case, `]`) and one actually intended to end the string.
Like Swift and Rust, we attach a run of *disambiguation characters* to the opening and closing tokens.
In our case, we use an arbitrary amount of minus characters matching on both ends.

Previously I experimented with using hash instead of minus characters inside square brackets.
This would be convenient because the hash character would otherwise be useless to put at the start of Python code.
I found this created too much visual noise inside the square brackets.

Minus characters are placed inside, not outside, the square brackets.
An unfortunate consequence of this is sequences like `[-1]` look valid but aren't - although syntax highlighting should help in that case.
I decided to place minus characters inside because when they are outside (i.e. participating in text) they expand to hyphens or dashes.
In order to place a hyphen or dash directly before something from code, the sequence `---[something in code]` is necessary.
If minuses outside the square brackets were included in the code token, the only way to avoid this would be to escape the minuses, e.g. `--\-[something in code]`, but then the escaped minus wouldn't be merged into a hyphen.

Minuses are used inside the square brackets so that it is clear the minuses are disambiguation characters instead of text.

The other possible permutation is hashes outside the square brackets, but that would cause inconsistency when mixing eval-brackets with raw scopes e.g. `###[blah]####{raw content}#`.

## Scopes

Scopes are opened with the squiggly brace character `{`.
There are three kinds of scope:
- If the scope is followed by (optional) whitespace, (optional) a comment, and then a newline, it opens a block scope.
    - The parser will enter block mode inside the scope.
    - You can create `Paragraph`s by writing text, `BlockScope`s by opening new block scopes inside, and other `Block` instances with eval-brackets.
        - All these objects will be put into a `BlockScope` Python object.
    - The scope must be closed by a closing squiggly brace `}` outside of a paragraph.
    - Closing the scope emits a `BlockScope` Python object into the document.
    - If this scope was opened directly after an eval-bracket, the interpreter looks for a method `build_from_blocks()` on the evaluated object and calls that method with the `BlockScope` it just created.
        - If the evaluated object doesn't have a `build_from_blocks()` method a `TypeError` is thrown.
        - The return value of that method is then emitted into the document instead of the `BlockScope`.
- If the scope is followed by (optional) whitespace and then non-whitespace content *before* a newline, it opens an inline scope.
    - The parser will enter inline mode inside the scope.
    - In inline mode, you can create `Text` by writing whitespace and non-whitespace content, `Raw` by opening new raw scopes, `InlineScope`s by opening new inline scopes, and other `Inline` instances with eval-brackets.
        - All these objects will be put into a `InlineScope` Python object.
    - The scope must be closed with a squiggly brace on the same line `}`.
    - Closing the scope emits an `InlineScope` Python object into the document.
    - If the scope was opened directly after an eval-bracket, the interpreter looks for a method `build_from_inlines()` on the evaluated object and calls that method with the `InlineScope` it just created.
        - If the evaluated object doesn't have a `build_from_inlines()` method a `TypeError` is thrown.
        - The return value of that method is then emitted into the document instead of the `InlineScope`.
- If the scope is *preceded* by N hashes, it opens a raw scope.
    - All text between the open and close is taken raw, directly from the document, and packed into a Python string
        - With the exception of newlines `\r` and `\r\n` which are normalized to `\n`.
    - The scope must be closed a squiggly brace close `}` followed by N hashes `#`.
    - Closing the scope emits a `Raw` Python object into the document.
        - `Raw` is an instance of `Inline`, so opening a raw scope in block mode will implicitly create a new Paragraph starting with the `Raw` object and enter inline mode.
    - If this scope was opened directly after an eval-bracket, the interpreter looks for a method `build_from_raw()` on the evaluated object and calls that method with the Python string it just created.
        - If the evaluated object doesn't have a `build_from_raw()` method a `TypeError` is thrown.
        - Note: It does not call the method with an actual `Raw` instance, just the string that would be inside.
        - The return value of that method is then emitted into the document instead of the `Raw`.

### A note on the placement of raw-scope hashes

I experimented with having the N hashes inside the raw-scope open and close instead of on the outside.
This would make the parsing more consistent with Block/Inline scopes, which are disambiguated by the newlines inside, and with code.
However, I felt the inner hashes made it more confusing where the raw capture began, and in this context that's the most important thing, e.g.

```
{###rawstuff###} vs #{rawstuff}#
```

## Hyphens

TODO

## Newlines in raw scopes and code

Newlines can be inconsistent between operating systems, and handling some combination of `\r`, `\n`, and `\r\n` is a must.
turnip_text captures each of these as a Newline token, so supports all of them.
Newlines are the only exception to raw string capture, as used in raw scopes and eval-brackets, and are always converted to `\n` before exposing them to Python.
This means all newlines captured in raw scopes, and all newlines inside eval-brackets, are captured

## Newlines in block mode

There was a substantial amount of debate between requiring a *blank* line between block-mode elements, and a *new* line.
There must be at least a new line between block elements, because otherwise certain situations would be misleading:

```
[SomeBlock()] could be followed by a paragraph, creating two separated blocks in the output \
even though in the source code they are adjacent and [emph]{seem} like one paragraph!
```

The whole point is to make sure separate blocks are visually separated in the source code.
Paragraphs are in some ways a special case and the justification for expecting a *blank* line, because paragraphs already require a fully blank line to end.
Requiring a blank line over a newline sounds like a good idea in simple cases:

```
# These look visually grouped together... kind of
[SomeHeader()]
[SomeBlock()]


# Aaah, much better!
[SomeHeader()]

[SomeBlock()]
```

But consider the case of adjacent block scopes!

```
{
    Stuff in scope one
} # Even with the adjacent line, there's clearly enough visual separation!
{
    Stuff in scope two
}
```

Well fine, we could change the rules: after emitting a block scope, the requirement is a new line, not a blank line.
But then we arrive at code-built-from-block-scopes:

```
[itemize]{
    [item]{
        First item
    }
    [item]{ # Uh oh...
        Second item
    }
}
```

The parser can't tell the difference between a block-emitted-from-code-owning-a-block-scope and a block-emitted-from-code, so at best you could say after a block scope or code-emitting-block or code-emitting-header the requirement is a new line - but that's almost everything!
Those rules feel inconsistent for code-emitting-source.
If you could filter out specifically block-or-header-emitted-from-code-owning-a-block-scope, then maybe it could work?

```
[itemize]{
    [item]{
        ...   
    }
}
Wow some content that's not separated :)
```

Ah, bugger.
Effectively the rule doesn't require blank lines because any rules I come up with that feel right would be hyper-specific, complex to implement, and ultimately it's all down to taste anyway.
The case of emitting multiple blocks on the *same* line is always misleading, so that isn't allowed, but there's enough wiggle room for adjacent lines that it's better to allow them in all cases then get allowing/disallowing them wrong.