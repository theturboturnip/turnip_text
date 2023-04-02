import abc
from os import PathLike
from typing import Any, Callable, Dict, Generic, Iterable, Iterator, List, Mapping, Optional, Protocol, Tuple, Type, TypeVar, Union, runtime_checkable

from turnip_text import Block, BlockScope, BlockScopeBuilder, Inline, InlineScopeBuilder, parse_file_native
from turnip_text.turnip_text import InlineScope, Paragraph, Sentence, UnescapedText

CiteKey = str
CiteKeyWithNote = Tuple[CiteKey, str]


def parse_file(p: PathLike, r: 'Renderer') -> BlockScope:
    # TODO this seems super icky
    # The problem: we want to be able to call e.g. [footnote] inside the turniptext file.
    # But if footnote were a free function, it would mutate global state -- we don't want that.
    # A hack! Require a 'renderer object' to be passed in - this encapsulates the local state.
    # Create a new dictionary, holding all of the Renderer's public fields,
    # and use that as the locals for parse_file_local.
    
    return parse_file_native(str(p), dictify_renderer(r))

class dictify_pure_property(property):
    """Equivalent to `property`, but acts as a purity marker.
    This should only be used if invoking the property itself DOESN'T MUTATE STATE.
    This means calling it once is equivalent to calling it many times, which is a useful property when dictifying."""
    pass

def dictify_renderer(r: 'Renderer') -> Dict[str, Any]:
    """
    Given an object implementing `Renderer`, get a dict of all functions, methods, and fields it exposes publically.

    Public = does not begin with '_'. This hides internal Python methods (e.g. `__str__`), name-mangled variables (both the original var `__name_mangled` and the mangled `_ExampleClass__name_mangled`, and any variables the programmer doesn't wish to expose `_plz_dont_modify_directly`.

    These are retrieved as follows:
    1. Get `dir(r)`, to get a dictionary of the fields and descriptors it exposes.
    2. Iterate through the keys, filtering out any that begin with `_` to find the public fields.
    3. use `getattr(r, key)` to get the values of those fields and descriptors, (which could be bound methods, static methods, or plain values), putting them into a new dictionary, which is returned.
        a. Warn the user if `type(r).__dict__[key]` is an impure DATA DESCRIPTOR, e.g. a property. These are evaluated ONCE inside this function, and wouldn't be repeatedly evaluated when using the returned dict.
        That is different to the usual behaviour: if `key` is a property, reading `r.key` will call `type(r).__dict__[key].__get__(...)` every time. `returned_dict[key]` holds the value returned from calling that ONCE, and repeatedly reading it will not re-invoke the property getter.
        IF THE PROPERTY IS PURE you can avoid this warning by using @dictify_pure_property to declare it.

    This can be used as the execution environment for code inside a turnip_text file.

    [1]: Information on Python "descriptors" https://docs.python.org/3.8/howto/descriptor.html
    """

    from inspect import isdatadescriptor

    r_obj_public_fields: List[str] = [
        k
        for k in dir(r)
        if not k.startswith("_")
    ]
    
    r_type_dict: Mapping[str, Any] = type(r).__dict__
    
    # Warn about impure data descriptor fields
    for k in r_obj_public_fields:
        if isdatadescriptor(r_type_dict[k]) and not isinstance(r_type_dict[k], dictify_pure_property):
            print(f"dictify_renderer Warning: renderer {r} exposes a public 'data descriptor' (e.g. a property) "
                  f"named {k!r}. This will be evaluated exactly once, and the result will be stored in the "
                  f"returned dict, instead of evaluating the property each time the dict is accessed. "
                  f"DO NOT USE THESE IF YOU CAN AVOID IT. Use a normal field instead.")

    return {
        k: getattr(r, k)
        for k in r_obj_public_fields
    }
    


@runtime_checkable
class Renderer(Protocol):
    """
    Renderers should export a consistent set of functions for documents to rely on.

    These should only use 'pure' properties, that don't mutate state, because they won't be called multiple times - see `dictify_renderer()` function for details why.
    'Pure' properties need to be marked with `@dictify_pure_property`.

    If you want impure results (i.e. once that mutate state) without function call syntax, you can use Builders.
    e.g. the `footnote` property is pure, but returns a single builder object that mutates the internal list of footnotes whenever `.build()` is called.
    """
    
    # Inline formatting
    @dictify_pure_property
    def emph(self) -> InlineScopeBuilder:
        ...

    # Citations
    def cite(self, *keys: Union[CiteKey, CiteKeyWithNote]) -> Inline: ...
    def citeauthor(self, key: CiteKey) -> Inline: ...

    # Footnotes
    @dictify_pure_property
    def footnote(self) -> InlineScopeBuilder:
        """Define an unlabelled footnote, capturing the following inline text as the contents,
        and insert a reference to that footnote here.
        
        `[footnote]{contents}` is roughly equivalent to `[footnote_ref(<some_label>)]` and `[footnote_text(<some_label>)]{contents}`"""
        ...
    def footnote_ref(self, label: str) -> Inline:
        """Insert a reference to a footnote, which will have the contents defined by a matching call to footnote_text.
        If, at render time, footnote_text has not been called with a matching label then an error will be raised."""
        ...
    def footnote_text(self, label: str) -> BlockScopeBuilder:
        """Define the contents of a footnote with the given label. Cannot be called twice with the same label."""
        ...

    # Structure
    def section(self, name: str, label: Optional[str] = None, numbered: bool = False) -> BlockScopeBuilder: ...
    def subsection(self, name: str, label: Optional[str] = None, numbered: bool = False) -> BlockScopeBuilder: ...
    def subsubsection(self, name: str, label: Optional[str] = None, numbered: bool = False) -> BlockScopeBuilder: ...

    # Figures?

T = TypeVar('T')
CustomRenderFunc = Callable[['ExperimentalActualRenderer', T], str]
class TypeToRenderMap(Generic[T], abc.ABC):
    handlers: Dict[Type[T], CustomRenderFunc]
    default_render: CustomRenderFunc

    def __init__(self, default_render: CustomRenderFunc) -> None:
        super().__init__()
        self.handlers = {}
        self.default_render = default_render

    def push_association(self, t: Type[T], func: CustomRenderFunc):
        self.handlers[t] = func

    def render(self, base_renderer: 'ExperimentalActualRenderer', obj: T) -> str:
        for t, renderer in self.handlers.items():
            if isinstance(obj, t):
                return renderer(base_renderer, obj)
        return self.default_render(base_renderer, obj)

class ExperimentalActualRenderer(abc.ABC):
    PARAGRAPH_SEP: str
    SENTENCE_SEP: str

    plugins: List['ExperimentalRendererPlugin']

    block_handlers: TypeToRenderMap[Block]
    inline_handlers: TypeToRenderMap[Inline]

    def __init__(self, plugins: List['ExperimentalRendererPlugin']) -> None:
        super().__init__()

        # Create handlers and 
        self.block_handlers = TypeToRenderMap(lambda _, __: raise NotImplementedError("Need default block handler?"))
        self.block_handlers.push_association(BlockScope, lambda _, bs: self.render_blockscope(bs))
        self.block_handlers.push_association(Paragraph, lambda _, bs: self.render_paragraph(bs))

        self.inline_handlers = TypeToRenderMap(lambda _, __: raise NotImplementedError("Need default inline handler?"))
        self.inline_handlers.push_association(InlineScope, lambda _, inls: self.render_inlinescope(inls))
        self.inline_handlers.push_association(UnescapedText, lambda _, t: self.render_unescapedtext(t))

        self.plugins = plugins
        for p in self.plugins:
            for (t, func) in p.block_handlers():
                self.block_handlers.push_association(t, func)
            for (t, func) in p.inline_handlers():
                self.inline_handlers.push_association(t, func)

    def render_unescapedtext(self, t: UnescapedText) -> str:
        """The baseline - take text and return a string that will look like that text exactly in the given backend."""
        raise NotImplementedError(f"Need to implement render_unescapedtext")
    
    def render_doc(self, doc: BlockScope) -> str:
        # TODO: Document prefix/postfix?
        return self.render_blockscope(doc)

    def render_inline(self, i: Inline) -> str:
        return self.inline_handlers.render(self, i)
    
    def render_block(self, b: Block) -> str:
        return self.block_handlers.render(self, b)
    
    def render_blockscope(self, bs: BlockScope) -> str:
        # Default: join paragraphs with self.PARAGRAPH_SEP
        # If you get nested blockscopes, this will still be fine - you won't get double separators
        return self.PARAGRAPH_SEP.join(
            self.render_block(b)
            for b in bs
        )
    
    def render_paragraph(self, p: Paragraph) -> str:
        # Default: join sentences with self.SENTENCE_SEP
        return self.SENTENCE_SEP.join(self.render_sentence(s) for s in p)

    def render_inlinescope(self, inls: InlineScope) -> str:
        # Default: join internal inline elements directly
        return "".join(
            self.render_inline(i)
            for i in inls
        )
    
    def render_sentence(self, s: Sentence) -> str:
        # Default: join internal inline elements directly
        # TODO could be extended by e.g. latex to ensure you get sentence-break-whitespace at the end of each sentence?
        return "".join(
            self.render_inline(i)
            for i in s
        )

class ExperimentalRendererPlugin:
    name: str

    def block_handlers(self) -> Iterable[Tuple[Type[Block], CustomRenderFunc]]:
        return ()
    def inline_handlers(self) -> Iterable[Tuple[Type[Inline], CustomRenderFunc]]:
        return ()
    