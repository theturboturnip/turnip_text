[AsciiDoc syntax references](https://docs.asciidoctor.org/asciidoc/latest/syntax-quick-reference/#ids-roles-and-options)

[Cheat sheet](https://powerman.name/doc/asciidoc)

[Playground](https://asciidoclive.com/edit/scratch/1)

# Conclusions
## The Good
- paragraph-level control (e.g. `.lead`, `[%hardbreaks]`)
- paragraph-level titling
- built-in paragraph handling
- special end-of-line syntax 
  - asciidoc uses plus-at-end-of-line to force a line break
  - shows familiarity with concept, using it for sentence continuation could be good
## The Bad
- first-class URLs, url formatting
  - encourages plain urls at the top level which are magically picked up and converted
- literal paragraphs
  - indent is useful for logical structuring, entering "code/monospace mode" should be separate & explicit. backticks are fine for this
- inconsistency of first-class concepts
  - e.g. types of block
  - asterisk blocks just don't work in the playground link
  - if you build concepts into your syntax, make sure everyone bloody well supports them
  - if they don't, raise a warning or error!

# Basic Structure

## Paragraphs
```asciidoc
Paragraphs don't require special markup in AsciiDoc.
A paragraph is defined by one or more consecutive lines of text.
Line breaks within a paragraph are not displayed.

Leave at least one empty line to begin a new paragraph.
```

### Literal Paragraphs i.e. code formatting
```asciidoc
A normal paragraph.

 A literal paragraph.
 One or more consecutive lines indented by at least one space.

 The text is shown in a fixed-width (typically monospace) font.
 The lines are preformatted (i.e., as formatted in the source).
 Spaces and newlines,
 like the ones in this sentence,
 are preserved.
```
becomes


A normal paragraph.

```
A literal paragraph.
One or more consecutive lines indented by at least one space.
```
```
The text is shown in a fixed-width (typically monospace) font.
The lines are preformatted (i.e., as formatted in the source).
Spaces and newlines,
like the ones in this sentence,
are preserved.
```

### Forced line breaks
```
Roses are red, +
violets are blue.

[%hardbreaks]
A ruby is red.
Java is black.
```
The plus symbol inserts a manual linebreak
and the `[%hardbreaks]` forces all linebreaks in the paragraph to manual

### "Lead paragraphs"
```
[.lead]
This text will be styled as a lead paragraph (i.e., larger font).

This paragraph will not be.
```

### Types of blocks
Blocks can have titles.

```
.Title of the block
****
content of the block?? this doesn't actually work in the playground
****
```

Blocks are defined by a string of dashes (code formatting), equals (put in a box), asterisk (put in a differently shaded box? a "sidebar block"?), underscores (block quotes), two dashes (normal block of text, which you can change to any other type by putting a header on)

```
[quote,"Charles Dickens","A Tale of Two Cities"]
It was the best of times, it was the worst of times, it was the age of wisdom,
it was the age of foolishness...

[quote,Abraham Lincoln,Address delivered at the dedication of the Cemetery at Gettysburg]
____
Four score and seven years ago our fathers brought forth
on this continent a new nation...
____

[quote]
--
An open block can be an anonymous container,
or it can masquerade as any other block.
--
```
the above are all block quotes

```
-----------------
#!/usr/bin/env python
import antigravity
try:
  antigravity.fly()
except FlytimeError as e:
  # um...not sure what to do now.
  pass
-----------------

[source,python]
-----------------
#!/usr/bin/env python
import antigravity
try:
  antigravity.fly()
except FlytimeError as e:
  # um...not sure what to do now.
  pass
-----------------

[quote,"Charles Dickens","A Tale of Two Cities"]
--
#!/usr/bin/env python
import antigravity
try:
  antigravity.fly()
except FlytimeError as e:
  # um...not sure what to do now.
  pass
--
```
From the top: python in code format without syntax highlighting, python in code with syntax, text of python as a quote from A Tale of Two Cities.

Can also do a passthrough block

```
++++
<p>
Content in a passthrough block is passed to the output unprocessed.
That means you can include raw HTML, like this embedded Gist:
</p>

<script src="https://gist.github.com/mojavelinux/5333524.js">
</script>
++++
```

## Inline Styling
```
* normal, _italic_, *bold*, +verb+, `code`.
* "`double quoted`", '`single quoted`'.
* normal, ^super^, ~sub~.
* `passthru *bold*`
* #highlight#
```

verb != code, verb is just "ignore other formatting within this"

Styling is inherently inline, styling is inherently *composable*

Most permutations of X inside Y are allowed, except for super/sub which have some special rules i.e. no spaces allowed.
Also `*bold*` can't be preceded by words - `semi*bold*` doesn't work.

semi**bold** *does* work in markdown, tho

```
^super*bold*?^ no

^super *bold*? _italic_? `code`? +verb *notbold*+?^ no, spaces

^*bold*?_italic_?`code`?+verb *notbold*+?super^ yes

^super with spaces?^ no

^supernospaces?^ yes
```
(and the same with subscript)

Proper quotes require tight bounds, and also require both sides

```
"`quote`" yes
"` quote `" no
"` quote`" no
"`open quote no
```

Single quotes are a bit more flexible
```
thing`'s yes
thing's  yes, equivalent to before
'`squot`' yes
'` topquot no
```

## Links
```
https://asciidoctor.org - automatic!

https://asciidoctor.org[Asciidoctor]

https://chat.asciidoc.org[Discuss AsciiDoc,role=external,window=_blank]

devel@discuss.example.org

mailto:devel@discuss.example.org[Discuss]

mailto:join@discuss.example.org[Subscribe,Subscribe me,I want to join!]

link:++https://example.org/?q=[a b]++[URL with special characters]

https://example.org/?q=%5Ba%20b%5D[URL with special characters]
```
ew

`link:` is a "macro prefix"?

### Crossrefs
```
See <<paragraphs>> to learn how to write paragraphs.

Learn how to organize the document into <<section-titles,sections>>.
```
(within one document)

```
Refer to xref:document-b.adoc#section-b[Section B of Document B] for more information.

If you never return from xref:document-b.adoc[Document B], we'll send help.
```
(inter-document)

## substitutions (TODO)