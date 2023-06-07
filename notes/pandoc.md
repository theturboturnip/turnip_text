Pandoc has a Lua API with a set of types available:

```
Meta = Mapping[str, MetaValues]
MetaValue = 
    MetaBool | 
    MetaString | 
    MetaInline | 
    MetaBlock | 
    MetaList | 
    MetaStrToStrMap

Block =
    BlockQuote |
    BulletList | 
    CodeBlock |
    DefinitionList |
    Div |
    Figure |
    Header |
    HorizontalRule |
    LineBlock |
    OrderedList |
    Para |
    Plain |
    RawBlock |
    Table

Inline = 
    Cite |
    Code |
    Emph |
    Image |
    LineBreak |
    Link |
    Math |
    DisplayMath | # Deprecated
    InlineMath | # Deprecated
    Note |
    Quoted |
    SingleQuoted | # Deprecated
    DoubleQuoted | # Deprecated
    RawInline |
    SmallCaps |
    SoftBreak |
    Space |
    Span |
    Str |
    Strikeout |
    Strong | 
    Subscript |
    Superscript |
    Underline
```

I want to be able to emit constructs from code without knowing which backend is in use ex.
```python
qemu = Code(Text("QEMU"))

# In turnip_text
I like to use [qemu]
```
```latex
# In latex
I like to use \texttt{QEMU}
```
```md
# In Markdown
I like to use `QEMU`
```
but I also want to be able to swap in/out the things that render those at an individual level.

ALTHOUGH
need to debate how to interface with it. could also do

```python
qemu = ctx.code.build_from_inlines("QEMU")
# with coersion "QEMU" -> UnescapedText("QEMU") -> [UnescapedText("QEMU")]

# without coersion (now)
qemu = ctx.code.build_from_inlines([UnescapedText("QEMU")])
```

which moves the abstraction to the function level (good, allows extra attrs with `*args, **kwargs`)

but that's not a great syntax?

I _also_ want to be able to add new "primitives". A single Table type may not be sufficient, for example, and when I'm iterating I might want to add a new Url type with a name :).

