import abc
from os import PathLike
from typing import Callable, Dict, Generic, Iterable, List, Tuple, Type, TypeVar

from turnip_text import Block, BlockScope, Inline, parse_file_native
from turnip_text.renderers.dictify import dictify
from turnip_text.turnip_text import InlineScope, Paragraph, Sentence, UnescapedText

T = TypeVar('T')
CustomRenderFunc = Tuple[Type[T], Callable[['Renderer', T], str]]
class TypeToRenderMap(Generic[T], abc.ABC):
    handlers: Dict[Type[T], Callable[['Renderer', T], str]]

    def __init__(self) -> None:
        super().__init__()
        self.handlers = {}

    def push_association(self, t_func: CustomRenderFunc):
        t, func = t_func
        self.handlers[t] = func

    def render(self, base_renderer: 'Renderer', obj: T) -> str:
        for t, renderer in self.handlers.items():
            if isinstance(obj, t):
                return renderer(base_renderer, obj)
        raise NotImplementedError(f"Couldn't handle {obj}")


class Renderer(abc.ABC):
    PARAGRAPH_SEP: str
    SENTENCE_SEP: str

    plugins: List['RendererPlugin']

    block_handlers: TypeToRenderMap[Block]
    inline_handlers: TypeToRenderMap[Inline]

    def __init__(self, plugins: List['RendererPlugin']) -> None:
        super().__init__()

        # Create handlers and 
        self.block_handlers = TypeToRenderMap()
        self.block_handlers.push_association((BlockScope, lambda _, bs: self.render_blockscope(bs)))
        self.block_handlers.push_association((Paragraph, lambda _, bs: self.render_paragraph(bs)))

        self.inline_handlers = TypeToRenderMap()
        self.inline_handlers.push_association((InlineScope, lambda _, inls: self.render_inlinescope(inls)))
        self.inline_handlers.push_association((UnescapedText, lambda _, t: self.render_unescapedtext(t)))

        self.plugins = plugins
        for p in self.plugins:
            for block_t_func in p._block_handlers():
                self.block_handlers.push_association(block_t_func)
            for inl_t_func in p._inline_handlers():
                self.inline_handlers.push_association(inl_t_func)

    def parse_file(self, p: PathLike) -> BlockScope:
        # TODO this seems super icky
        # The problem: we want to be able to call e.g. [footnote] inside the turniptext file.
        # But if footnote were a free function, it would mutate global state -- we don't want that.
        # A hack! Require a 'renderer object' to be passed in - this encapsulates the local state.
        # Create a new dictionary, holding all of the Renderer's public fields,
        # and use that as the locals for parse_file_local.
        
        locals = {}
        for plugin in self.plugins:
            locals.update(dictify(plugin))

        return parse_file_native(str(p), locals)


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

class RendererPlugin(abc.ABC):
    """
    Plugins should export a consistent set of functions for documents to rely on.

    These should only use 'pure' properties, that don't mutate state, because they won't be called multiple times - see `dictify_renderer()` function for details why.
    'Pure' properties need to be marked with `@dictify_pure_property`.

    If you want impure results (i.e. once that mutate state) without function call syntax, you can use Builders.
    e.g. the `footnote` property is pure, but returns a single builder object that mutates the internal list of footnotes whenever `.build()` is called.
    """
        
    @property
    def _plugin_name(self) -> str:
        return type(self).__name__

    def _block_handlers(self) -> Iterable[CustomRenderFunc[Block]]:
        return ()
    def _inline_handlers(self) -> Iterable[CustomRenderFunc[Inline]]:
        return ()