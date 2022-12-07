# Opinionated Text Formatting

Typesetting is complicated.

Problems with LaTeX
- hyphen/en dash/em dash
  - Most people don't know the difference, and it can be automatically detected!
  - If the expectation of a typesetting program is to produce nice output by default, it should autodetect that.
- having to escape "normal" characters
  - UNDERSCORE IS THE BANE OF MY EXISTENCE
- unicode/color unicode support
- left/right quotes
  - not so much a problem to use, but it lets people use literal `"` when that always looks bad
- automatic space detection
  - I didn't even know latex did this until a week ago
  - it uses a heuristic to determine where the end of a sentence is
    - "any space after a period terminates a sentence unless preceded by an uppercase letter"
  - it isn't even very good
    - screws up common abbrevations like e.g., Dr., et al.

Good thing about LaTeX
- User control!
  - user can select hyphen/en dash/em dash
  - user can override space heuristic with no-break, normal, and force-sentence-end spaces.
    - also select "french spacing" to disable the difference entirely
    - `\ ` = force normal space
    - `\@` = end the sentence after the next punctuation
    - `@` = non-breaking space


Fixes
- "smart dashes"
- have the backend do the escaping for you
  - except for code characters `[]{}#`
- unicode/unicode color????
- quote solutions
  - built-in `[quote], [squote], [dquote]` environments?
  - compiler warning for ""?
    - warning for ' would break apostrophes
  - "smart quotes" mode?
    - word-esque
    - could get stuff wrong
  - keep latex \`\` and '' chars
- make sentence detection easier
  - force one-sentence-per-line!
    - it's better for diffs
  - probably want newline backslash-escaping, to say "the sentence continues"
    - equivalent to latex backslash-escaping a space
  - ONLY newlines in the source create latex-style spaces
    - => don't need `\ ` or `\@`
    - BUT warn that if the line doesn't end with punctuation, no way to force LaTeX to insert a sentence-space at that point
  - could warn if we think line has a partial sentence i.e. if it doesn't end with punctuation
    - compile error?? `-Werror` mode??
  - could warn if we think someone has two sentences on one line, but that would have to use a heuristic false-positives
  - we can't do automatic non-breaking space, although we can remove the need for them in some places
    - e.g. `cleverref` style reference API
    - so still have `~`