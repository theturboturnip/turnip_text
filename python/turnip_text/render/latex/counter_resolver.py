from dataclasses import dataclass
from typing import Dict, Iterable, List, Optional, Tuple, Union

from turnip_text.render.counters import (
    CounterLink,
    CounterState,
    build_counter_hierarchy,
    counter_hierarchy_dfs,
    map_counter_hierarchy,
    resolve_counter_links,
)
from turnip_text.render.latex.backrefs import (
    LatexBackrefMethod,
    LatexCleveref,
    LatexHyperlink,
    LatexPageRef,
)
from turnip_text.render.latex.renderer import (
    LatexBackrefMethodImpl,
    LatexCounterFormat,
    LatexCounterSpec,
)


@dataclass
class LatexCounterDecl:
    provided_by_docclass_or_package: bool
    """Does any package or the documentclass define this counter already? If not, we need to declare it with newcounter"""

    default_reset_latex_counter: Optional[str]
    """If this was provided_by_docclass_or_package, what is the standard 'reset counter' for this counter?"""

    default_fmt: LatexCounterFormat
    """A LatexCounterFormat encoding of how LaTeX normally renders this counter"""


@dataclass
class ResolvedTTAndLatexCounters:
    """The result of resolving a LatexCounterSetup after all plugins have made their requests."""

    tt_counters: CounterState
    """The hierarchy of turnip_text counters, stored in a CounterState ready for incrementing and tracking counter values.
    
    Magic turnip_text counters are stored as if they have no parent (e.g. are parented to the top level) and should not be incremented."""
    magic_tt_counter_to_latex_counter: Dict[str, str]
    """Mapping of (magic turnip_text counter) to (magic latex counter)"""
    latex_renderable_counters: List[LatexCounterSpec]
    """The final set of LaTeX counters, structured in a way the LatexRenderer can understand."""
    backref_impls: List[LatexBackrefMethodImpl]
    """A list of unique backref implementations used by the counters in `latex_counters`.
    Each element appears once, but this isn't a set because LatexBackrefMethodImpl doesn't need to be hashable."""


class LatexCounterResolver:
    """This class determines the relationship of turnip_text counters to LaTeX counters,
    and how each counter should be backreferenced.
    Plugins can declare which LaTeX plugins exist, and create new turnip_text counters to map to them.
    Each LaTeX counter maps to either one or zero turnip_text counters.

    RenderPlugins declare counters through this module in two ways:
    - The normal way, for counters that increment in a predictable way that turnip_text can match
        1. declare_latex_counter() to establish how LaTeX initially perceives the counter
        2. [optional] request_latex_counter_fmt() to override how the LaTeX counter is formatted
        3. declare_tt_counter(<turnip_text counter>, <latex counter>) to declare a turnip_text counter that maps to a LaTeX counter
        4. [optional] request_tt_counter_parent(<turnip_text counter>, <turnip_text parent counter>) request that one tt counter is reset when another tt counter (the 'parent') increments.
    - Magic counters, for counters like the page counter and footnote counter that increment in ways turnip_text can't predict
        1. declare_magic_tt_and_latex_counter() to simultaneously declare the LaTeX counter and the turnip_text counter it maps to.
    """

    declared_latex_counters: Dict[str, LatexCounterDecl]
    """The non-magic LaTeX counters that have been declared"""
    latex_counter_override_fmt: Dict[str, LatexCounterFormat]
    """Overridden formatting for non-magic LaTeX counters. Backref methods must configure themselves to use these."""
    latex_counter_backref_method: Dict[str, Optional[LatexBackrefMethod]]
    """The backref method for each non-magic LaTeX counters. Will always be a key to backref_impls."""

    tt_counter_links: List[CounterLink]
    """Requested (parent, child) links in the turnip_text counter hierarchy"""

    # turnip_text:latex counters are (0 or 1):1 - LaTeX counters may be declared but not used
    tt_counter_to_latex_counter: Dict[str, str]
    latex_counter_to_tt_counter: Dict[str, str]
    # Map turnip_text to LaTeX counters for "magic" counters that LaTeX steps by itself in a way turnip_text cannot replicate
    magic_tt_counter_to_latex_counter: Dict[str, str]

    backref_impls: Dict[LatexBackrefMethod, LatexBackrefMethodImpl]

    def __init__(
        self,
        counter_link_override: Optional[Iterable[CounterLink]],
        latex_counter_format_override: Optional[Dict[str, LatexCounterFormat]],
        legal_backref_methods: Optional[List[LatexBackrefMethod]],
        # TODO config for the backref methods
    ) -> None:
        self.declared_latex_counters = {}
        if latex_counter_format_override:
            self.latex_counter_override_fmt = latex_counter_format_override
        else:
            self.latex_counter_override_fmt = {}
        self.latex_counter_backref_method = {}

        if counter_link_override:
            self.tt_counter_links = list(counter_link_override)
        else:
            self.tt_counter_links = []
        self.tt_counter_to_latex_counter = {}
        self.latex_counter_to_tt_counter = {}
        self.magic_tt_counter_to_latex_counter = {}

        self.backref_impls = {
            LatexBackrefMethod.Cleveref: LatexCleveref(),
            LatexBackrefMethod.Hyperlink: LatexHyperlink(),
            LatexBackrefMethod.PageRef: LatexPageRef(),
        }
        if legal_backref_methods:
            # redefine backref_impls only with keys in legal_backref_methods
            self.backref_impls = {
                key: val
                for key, val in self.backref_impls.items()
                if key in legal_backref_methods
            }

    def declare_latex_counter(
        self,
        latex_counter: str,
        decl: LatexCounterDecl,
        backref_method: Union[
            None, LatexBackrefMethod, Tuple[LatexBackrefMethod, ...]
        ],  # Either one or multiple possible backref methods. If a tuple, the first element that is present in self.backref_impls will be selected
    ) -> None:
        if latex_counter in self.declared_latex_counters:
            raise RuntimeError(
                f"Tried to declare the LaTeX counter '{latex_counter}' twice!"
            )
        self.declared_latex_counters[latex_counter] = decl
        # Figure out which backref_method we can use
        if backref_method is not None:
            if isinstance(backref_method, LatexBackrefMethod):
                backref_methods: Tuple[LatexBackrefMethod, ...] = (backref_method,)
            else:
                backref_methods = backref_method
            found_valid_method = False
            for backref_method in backref_methods:
                if backref_method in self.backref_impls:
                    self.latex_counter_backref_method[latex_counter] = backref_method
                    found_valid_method = True
                    break
            if not found_valid_method:
                raise ValueError(
                    f"None of the supplied backref methods {backref_methods} for counter '{latex_counter}' were available in the document. Available methods: {self.backref_impls.keys()}"
                )
        else:
            self.latex_counter_backref_method[latex_counter] = None

    def request_latex_counter_fmt(
        self, latex_counter: str, override_fmt: LatexCounterFormat
    ) -> None:
        if latex_counter in self.latex_counter_override_fmt:
            raise RuntimeError(
                f"Tried to override formatting for LaTeX counter '{latex_counter}' twice!'"
            )
        self.latex_counter_override_fmt[latex_counter] = override_fmt

    def declare_magic_tt_and_latex_counter(
        self,
        tt_counter: str,
        latex_counter: str,
    ) -> None:
        if (
            tt_counter in self.tt_counter_to_latex_counter
            or tt_counter in self.magic_tt_counter_to_latex_counter
        ):
            raise ValueError(
                f"Tried to declare turnip_text counter {tt_counter} twice!"
            )
        if latex_counter in self.latex_counter_to_tt_counter:
            raise ValueError(
                f"Tried to declare LaTeX counter {latex_counter} as magic when it has already been declared as not-magic!"
            )
        self.magic_tt_counter_to_latex_counter[tt_counter] = latex_counter

    def declare_tt_counter(
        self,
        tt_counter: str,
        latex_counter: str,
    ) -> None:
        if (
            tt_counter in self.tt_counter_to_latex_counter
            or tt_counter in self.magic_tt_counter_to_latex_counter
        ):
            raise ValueError(
                f"Tried to declare turnip_text counter {tt_counter} twice!"
            )
        if latex_counter in self.latex_counter_to_tt_counter:
            raise ValueError(
                f"Tried to map two turnip_text counters {tt_counter} and {self.latex_counter_to_tt_counter[latex_counter]} to the same LaTeX counter {latex_counter}!"
            )
        self.tt_counter_to_latex_counter[tt_counter] = latex_counter
        self.latex_counter_to_tt_counter[latex_counter] = tt_counter

    def request_tt_counter_parent(
        self, tt_counter: str, tt_parent_counter: Optional[str]
    ) -> None:
        # Apply the requested counter links
        self.tt_counter_links.append((tt_parent_counter, tt_counter))

    def resolve_all(self) -> ResolvedTTAndLatexCounters:
        # We can now process the counters and constraints the plugins requested.
        latex_used_but_not_declared = set(
            latex_counter
            for latex_counter in self.latex_counter_to_tt_counter.keys()
            if latex_counter not in self.declared_latex_counters
        )
        if latex_used_but_not_declared:
            raise RuntimeError(
                f"The following LaTeX counters are used (associated with turnip_text counters) but not declared: {latex_used_but_not_declared}"
            )

        # There can be declared-but-not-used LaTeX counters, but every turnip_text counter maps to a LaTeX counter.
        # We can assume that only LaTeX counters mapped to turnip_text counters are actually used (excl. magic counters like 'footnote' and 'page')
        # => if we compute the counter hierarchy in terms of LaTeX counters, we can map it to turnip_text counters
        # instead of trying to convert all of the links -> turnip_text which may not be possible.
        # so if in LaTeX a -> b -> c and in turnip_text a = x, b = None, c = y you need to merge the children of b into the children of a while processing and get x -> y (equiv. to x -> None -> y)
        # *but* it will be correct - we can assume the turnip_text counters and the LaTeX counters will keep in sync, because we know the LaTeX counters without turnip_text counterparts will never be stepped - any child of a LaTeX counter with no counterpart may as well not have a parent.
        # We keep the parent in the LaTeX hierarchy because it may affect formatting and we don't want to make unnecessary changes to the LaTeX actual document hierarchy.

        # We also have the ability to register "magic" counters that can be stepped outside turnip_text and should not have children.
        # These counters can be stepped in turnip_text world but the values known to turnip_text may not be consistent with the ones known to LaTeX.
        # Examples include footnote and page.
        # They cannot be backreferenced through normal means, and they can't have non-magic children, because both cases may result in manually numbering based on the turnip_text value and producing inconsistent results to LaTeX.
        # It's also a bad idea to try and parent them, because that may require reparenting them in the LaTeX side which we can't do consistently.
        # There's no point in expressing the "magic" parenting hierarchy e.g. page -> footnote because turnip_text can't step either consistently.

        for parent, child in self.tt_counter_links:
            if parent in self.magic_tt_counter_to_latex_counter:
                raise ValueError(
                    f"Magic turnip_text counter '{parent}' cannot have children - requested '{child}'"
                )
            if child in self.magic_tt_counter_to_latex_counter:
                raise ValueError(
                    f"Can't change parent of magic turnip_text counter '{child}' to '{parent}' - magic counters cannot be parented"
                )

        full_latex_counter_links = [
            (
                decl.default_reset_latex_counter,
                latex_counter,
            )
            for latex_counter, decl in self.declared_latex_counters.items()
        ] + [
            (
                (
                    self.tt_counter_to_latex_counter[parent]
                    if parent is not None
                    else None
                ),
                self.tt_counter_to_latex_counter[child],
            )
            for parent, child in self.tt_counter_links
        ]
        latex_counter_hierarchy = build_counter_hierarchy(
            full_latex_counter_links, set(self.declared_latex_counters.keys())
        )
        tt_counter_hierarchy = map_counter_hierarchy(
            latex_counter_hierarchy,
            lambda latex_counter: self.latex_counter_to_tt_counter.get(latex_counter),
        )
        # Add the magic turnip_text counters to the tt_hierarchy - LaTeX doesn't consider them, but internally we still need to be able to step them
        for magic_tt_counter in self.magic_tt_counter_to_latex_counter:
            tt_counter_hierarchy[magic_tt_counter] = {}
        tt_counters = CounterState(tt_counter_hierarchy)

        # We can also build a hierarchy of LaTeX counters, which we use to generate the final point-to-point relations
        # TODO don't do the work of resolving the counter links twice :(
        latex_counter_to_parent, _ = resolve_counter_links(
            full_latex_counter_links,
            set(self.latex_counter_to_tt_counter.keys()),
        )

        # Build the list of counters for Latex
        latex_renderable_counters = []
        used_backref_methods = set()
        for latex_counter in counter_hierarchy_dfs(latex_counter_hierarchy):
            decl = self.declared_latex_counters[latex_counter]

            backref_method = self.latex_counter_backref_method[latex_counter]
            if backref_method is not None:
                used_backref_methods.add(backref_method)

            tt_counter = self.latex_counter_to_tt_counter.get(latex_counter)

            latex_renderable_counters.append(
                LatexCounterSpec(
                    tt_counter=tt_counter,
                    latex_counter=latex_counter,
                    backref_impl=(
                        self.backref_impls[backref_method]
                        if backref_method is not None
                        else None
                    ),
                    provided_by_docclass_or_package=decl.provided_by_docclass_or_package,
                    default_reset_latex_counter=decl.default_reset_latex_counter,
                    reset_latex_counter=latex_counter_to_parent[latex_counter],
                    default_fmt=decl.default_fmt,
                    override_fmt=self.latex_counter_override_fmt.get(latex_counter),
                )
            )

        return ResolvedTTAndLatexCounters(
            tt_counters,
            self.magic_tt_counter_to_latex_counter,
            latex_renderable_counters,
            backref_impls=[
                self.backref_impls[backref_method]
                for backref_method in used_backref_methods
            ],
        )
