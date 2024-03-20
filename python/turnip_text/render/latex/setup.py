from typing import Dict, Iterable, List, Optional, Tuple, Union

from turnip_text import Block, DocSegmentHeader, Inline
from turnip_text.doc import DocSetup
from turnip_text.render import (
    EmitterDispatch,
    RenderPlugin,
    RenderSetup,
    VisitorFilter,
    VisitorFunc,
    Writable,
)
from turnip_text.render.counters import (
    CounterLink,
    CounterState,
    build_counter_hierarchy,
)
from turnip_text.render.latex.backrefs import (
    LatexBackrefMethod,
    LatexCleveref,
    LatexHyperlink,
    LatexPageRef,
)
from turnip_text.render.latex.renderer import LatexBackrefMethodImpl, LatexRenderer


# TODO this is a great place to put in stuff for calculating the preamble!
class LatexSetup(RenderSetup[LatexRenderer]):
    emitter: EmitterDispatch[LatexRenderer]
    counter_kind_to_backref_method: Dict[str, Optional[LatexBackrefMethod]]
    backref_impls: Dict[LatexBackrefMethod, LatexBackrefMethodImpl]
    requested_counter_links: List[CounterLink]
    counters: CounterState

    def __init__(
        self,
        plugins: Iterable[RenderPlugin[LatexRenderer, "LatexSetup"]],
        requested_counter_backref_methods: Dict[
            str, Union[None, LatexBackrefMethod, Tuple[LatexBackrefMethod, ...]]
        ] = {},
        requested_counter_links: Optional[Iterable[CounterLink]] = None,
        # TODO config for the backref methods
    ) -> None:
        super().__init__(plugins)
        self.emitter = LatexRenderer.default_emitter_dispatch()
        self.counter_kind_to_backref_method = {}
        self.backref_impls = {
            # TODO make sure we load hyperref and cleveref!
            # TODO make loading cleveref optional
            LatexBackrefMethod.Cleveref: LatexCleveref(),
            LatexBackrefMethod.Hyperlink: LatexHyperlink(),
            LatexBackrefMethod.PageRef: LatexPageRef(),
        }
        if requested_counter_links:
            self.requested_counter_links = list(requested_counter_links)
        else:
            self.requested_counter_links = []
        for counter, backref_method in requested_counter_backref_methods.items():
            self.define_counter_backref_method(counter, backref_method)
        # This allows plugins to register with the emitter and request specific counter links
        for p in plugins:
            p._register(self)
        # Now we know the full hierarchy we can build the CounterState
        self.counters = CounterState(
            build_counter_hierarchy(
                self.requested_counter_links,
                set(self.counter_kind_to_backref_method.keys()),
            )
        )

    def gen_dfs_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]]:
        vs: List[Tuple[VisitorFilter, VisitorFunc]] = [
            (None, self.counters.count_anchor_if_present)
        ]
        for p in self.plugins:
            v = p._make_visitors()
            if v:
                vs.extend(v)
        return vs

    def known_node_types(
        self,
    ) -> Iterable[type[Block] | type[Inline] | type[DocSegmentHeader]]:
        return self.emitter.renderer_keys()

    def known_countables(self) -> Iterable[str]:
        return self.counters.anchor_kind_to_parent_chain.keys()

    def define_counter_backref_method(
        self,
        counter: str,
        # counter_format: Optional[LatexCounterFormat],
        backref_method: Union[
            None, LatexBackrefMethod, Tuple[LatexBackrefMethod, ...]
        ],  # Either one or multiple possible backref methods. If a tuple, the first element that is present in self.backref_impls will be selected
    ) -> None:
        """
        Given a counter, define:
        - TODO how it's name is formatted in backreferences
        - what macros are used to backreference the counter
        """

        if counter in self.counter_kind_to_backref_method:
            return

        # Figure out which backref_method we can use
        if backref_method is not None:
            if isinstance(backref_method, LatexBackrefMethod):
                backref_methods: Tuple[LatexBackrefMethod, ...] = (backref_method,)
            else:
                backref_methods = backref_method
            found_valid_method = False
            for backref_method in backref_methods:
                if backref_method in self.backref_impls:
                    self.counter_kind_to_backref_method[counter] = backref_method
                    found_valid_method = True
                    break
            if not found_valid_method:
                raise ValueError(
                    f"None of the supplied backref methods {backref_methods} for counter '{counter}' were available in the document. Available methods: {self.backref_impls.keys()}"
                )
        else:
            self.counter_kind_to_backref_method[counter] = None

    def request_counter_parent(
        self, counter: str, parent_counter: Optional[str]
    ) -> None:
        # Apply the requested counter links
        self.requested_counter_links.append((parent_counter, counter))

    def to_renderer(self, doc_setup: DocSetup, write_to: Writable) -> LatexRenderer:
        return LatexRenderer(
            doc_setup,
            self.emitter,
            {
                counter: self.backref_impls[method]
                for counter, method in self.counter_kind_to_backref_method.items()
                if method is not None
            },
            write_to,
        )
