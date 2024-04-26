turnip-text is an opinionated language that aims for formatting in source files to map intuitively to document elements, with a minimum of hardcoded syntax.

## Code

Code can be opened anywhere in top-level, block, and inline mode.
Code is opened with a square bracket open `[` followed by N minus characters `-`, where N may be zero.
Code ends when N minus characters `-`, followed by a square bracket close `]`, are encountered.
The minuses can be used to disambiguate between a `]` for Python (e.g. closing a list) and the end of the code.
Text between the open and close (ignoring the minuses, but including newlines and *all whitespace and indentation*) is then captured and evaluated as Python code.
The open and close tokens are colloquially referred to as "eval-brackets".

For flexibility, the code can be evaluated in one of three ways:
- First, the code is stripped of whitespace on both ends and compiled as a single Python expression.
    - If this succeeds, the expression is evaluated and the resulting object is emitted into the document.
- If that throws a SyntaxError, the code is stripped of whitespace on both ends and compiled as a sequence of Python statements.
    - This allows multiline statements, function declarations etc. to be executed inside eval-brackets.
    - If this succeeds, the statements are executed and `None` is emitted into the document.
- TODO if that throws a SyntaxError, the code is *not* stripped of whitespace and instead surrounded in an `if True:\n` block before being compiled as a sequence of Python statements.
    - This is intended to allow consistently-indented Python code inside eval-brackets.
        - It may allow inconsistently-indented Python code inside eval-brackets, such as code which begins indented and on later lines is unindented. 
    - If this succeeds, the statements are executed and `None` is emitted into the document.
    - If this fails, the compile error is thrown into the enclosing Python context TODO with the source code attached.

### A note on the choice of disambiguation character

Eval-brackets are effectively a form of raw string that eventually get evaluated as Python, and need a disambiguation character to tell the difference between a string-end token inside Python (in our case, `]`) and one actually intended to end the string.
Like Swift and Rust, we attach a run of *disambiguation characters* to the opening and closing tokens.
In our case, we use an arbitrary amount of minus characters matching on both ends.

Previously I experimented with using hash instead of minus characters inside square brackets.
This would be convenient because the hash character would otherwise be useless to put at the start of Python code.
I found this created too much visual noise inside the square brackets.

Minus characters are placed inside, not outside, the square brackets.
An unfortunate consequence of this is sequences like `[-1]` look valid but aren't - although syntax highlighting should help in that case.
I decided to place minus characters inside because when they are outside (i.e. participating in text) TODO they expand to hyphens or dashes.
In order to place a hyphen or dash directly before something from code, the sequence `---[something in code]` is necessary.
If minuses outside the square brackets were included in the code token, the only way to avoid this would be to escape the minuses e.g. `--\-[something in code]` but then the escaped minus wouldn't be merged into a hyphen.

Minuses are used inside the square brackets so that it is clear the minuses are disambiguation characters instead of text.

The other possible permutation is hashes outside the square brackets, but that would cause inconsistency when mixing the two e.g. `###[blah]####{raw content}#`.

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
This would make the parsing more consistent with 

## Newlines

# Top-level, Block, Inline modes

The first evaluated file begins in toplevel mode.
At the toplevel, you can create `Header`s, `Block`s, and `TurnipTextSource` files in code.
