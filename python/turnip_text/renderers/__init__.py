import abc
from os import PathLike
from typing import (
    Callable,
    Dict,
    Generic,
    Iterable,
    Iterator,
    List,
    Tuple,
    Type,
    TypeVar,
)

from turnip_text import Block, BlockScope, Inline, parse_file_native
from turnip_text.renderers.dictify import dictify
from turnip_text.turnip_text import InlineScope, Paragraph, Sentence, UnescapedText

T = TypeVar("T")
CustomRenderFunc = Tuple[Type[T], Callable[["Renderer", T], str]]


class TypeToRenderMap(Generic[T]):
    _handlers: Dict[Type[T], Callable[["Renderer", T], str]]

    def __init__(self) -> None:
        super().__init__()
        self._handlers = {}

    def push_association(self, t_func: CustomRenderFunc):
        t, func = t_func
        if t in self._handlers:
            raise RuntimeError(f"Conflict: registered two renderers for {t}")
        self._handlers[t] = func

    def render(self, base_renderer: "Renderer", obj: T) -> str:
        for t, renderer in self._handlers.items():
            if isinstance(obj, t):
                return renderer(base_renderer, obj)
        raise NotImplementedError(f"Couldn't handle {obj}")


# TODO Make preamble/postamble return Blocks to be rendered instead of just str? Would allow e.g. a Bibliography section? Perhaps better to expose a bibliography block for "standard" postambles?
class AmbleMap:
    """Class that stores and reorders {pre,post}amble handlers.

    Handlers are simply functions that return a string, which have a unique ID.

    Only one handler per ID can exist.

    The user can request that the ID are sorted in a particular order, or default to order of insertion.

    When the document is rendered, the handlers will be called in that order."""

    _handlers: Dict[str, Callable[["Renderer"], str]]
    _id_order: List[str]

    def __init__(self) -> None:
        self._handlers = {}
        self._id_order = []

    def push_handler(self, id: str, f: Callable[["Renderer"], str]):
        if id in self._handlers:
            raise RuntimeError(f"Conflict: registered two amble-handlers for ID {id}")
        self._handlers[id] = f
        self._id_order.append(id)

    def reorder_handlers(self, selected_id_order: List[str]):
        """Request that certain handler IDs are rendered in a specific order.

        Does not need to be a complete ordering, i.e. if handlers ['a', 'b', 'c'] are registered
        this function can be called with ['c', 'a'] to ensure 'c' comes before 'a',
        but all of the IDs in the order need to have been registered.

        When the requested ordering is incomplete, handlers which haven't been mentioned
        will retain their old order but there is no specified ordering between (mentioned) and (not-mentioned) IDs.
        """

        assert all(id in self._handlers for id in selected_id_order)

        if len(selected_id_order) != len(set(selected_id_order)):
            raise RuntimeError(
                f"reorder_handlers() called with ordering with duplicate IDs: {selected_id_order}"
            )

        # Shortcut if the selected order is complete i.e. covers all IDs so far
        if len(self._id_order) == len(selected_id_order):
            self._id_order = selected_id_order
        else:
            # Otherwise, we need to consider the non-selected IDs too.
            # The easy way: put selected ones first, then non-selected ones last
            # Get the list of ids NOT in selected_id_order, in the order they're currently in in self._id_order
            non_selected_ids = [
                id for id in self._id_order if id not in selected_id_order
            ]
            self._id_order = selected_id_order + non_selected_ids

        assert all(id in self._id_order for id in self._handlers.keys())

    def generate_ambles(self, renderer: "Renderer") -> Iterator[str]:
        for id in self._id_order:
            yield self._handlers[id](renderer)


class Renderer(abc.ABC):
    PARAGRAPH_SEP: str
    SENTENCE_SEP: str

    plugins: List["RendererPlugin"]

    block_handlers: TypeToRenderMap[Block]
    inline_handlers: TypeToRenderMap[Inline]
    preamble_handlers: AmbleMap
    postamble_handlers: AmbleMap

    def __init__(self, plugins: List["RendererPlugin"]) -> None:
        super().__init__()

        # Create handlers and
        self.block_handlers = TypeToRenderMap()
        self.block_handlers.push_association(
            (BlockScope, lambda _, bs: self.render_blockscope(bs))
        )
        self.block_handlers.push_association(
            (Paragraph, lambda _, bs: self.render_paragraph(bs))
        )

        self.inline_handlers = TypeToRenderMap()
        self.inline_handlers.push_association(
            (InlineScope, lambda _, inls: self.render_inlinescope(inls))
        )
        self.inline_handlers.push_association(
            (UnescapedText, lambda _, t: self.render_unescapedtext(t))
        )

        self.preamble_handlers = AmbleMap()
        self.postamble_handlers = AmbleMap()

        self.plugins = plugins
        for p in self.plugins:
            for block_t_func in p._block_handlers():
                self.block_handlers.push_association(block_t_func)
            for inl_t_func in p._inline_handlers():
                self.inline_handlers.push_association(inl_t_func)
            for preamble_id, preamble_func in p._preamble_handlers():
                self.preamble_handlers.push_handler(preamble_id, preamble_func)
            for postamble_id, postamble_func in p._postamble_handlers():
                self.postamble_handlers.push_handler(postamble_id, postamble_func)

    def request_preamble_order(self, preamble_id_order: List[str]):
        self.preamble_handlers.reorder_handlers(preamble_id_order)

    def request_postamble_order(self, postamble_id_order: List[str]):
        self.postamble_handlers.reorder_handlers(postamble_id_order)

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

    def render_doc(self, doc_block: BlockScope) -> str:
        doc = ""
        for preamble in self.preamble_handlers.generate_ambles(self):
            doc += preamble
            doc += self.PARAGRAPH_SEP
        doc += self.render_blockscope(doc_block)
        for postamble in self.postamble_handlers.generate_ambles(self):
            doc += self.PARAGRAPH_SEP
            doc += postamble
        return doc

    def render_inline(self, i: Inline) -> str:
        return self.inline_handlers.render(self, i)

    def render_block(self, b: Block) -> str:
        return self.block_handlers.render(self, b)

    def render_blockscope(self, bs: BlockScope) -> str:
        # Default: join paragraphs with self.PARAGRAPH_SEP
        # If you get nested blockscopes, this will still be fine - you won't get double separators
        return self.PARAGRAPH_SEP.join(self.render_block(b) for b in bs)

    def render_paragraph(self, p: Paragraph) -> str:
        # Default: join sentences with self.SENTENCE_SEP
        return self.SENTENCE_SEP.join(self.render_sentence(s) for s in p)

    def render_inlinescope(self, inls: InlineScope) -> str:
        # Default: join internal inline elements directly
        return "".join(self.render_inline(i) for i in inls)

    def render_sentence(self, s: Sentence) -> str:
        # Default: join internal inline elements directly
        # TODO could be extended by e.g. latex to ensure you get sentence-break-whitespace at the end of each sentence?
        return "".join(self.render_inline(i) for i in s)


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

    def _preamble_handlers(self) -> Iterable[Tuple[str, Callable[[Renderer], str]]]:
        return ()

    def _postamble_handlers(self) -> Iterable[Tuple[str, Callable[[Renderer], str]]]:
        return ()
