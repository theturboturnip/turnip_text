from dataclasses import dataclass
from typing import Dict, Iterable, List, Optional, Tuple, Union, cast

from turnip_text import Block, DocSegment, Document, Header, Inline
from turnip_text.build_system import JobInputFile, JobOutputFile
from turnip_text.doc import DocSetup
from turnip_text.helpers import UNSET, MaybeUnset
from turnip_text.render import (
    EmitterDispatch,
    RenderPlugin,
    RenderSetup,
    VisitorFilter,
    VisitorFunc,
    Writable,
)
from turnip_text.render.counters import (
    CounterHierarchy,
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
    LatexPackageRequirements,
    LatexRenderer,
    LatexRequirements,
)
from turnip_text.render.manual_numbering import SimpleCounterFormat


@dataclass
class LatexCounterDecl:
    provided_by_docclass_or_package: bool
    """Does any package or the documentclass define this counter already? If not, we need to declare it with newcounter"""

    default_reset_latex_counter: Optional[str]
    """If this was provided_by_docclass_or_package, what is the standard 'reset counter' for this counter?"""

    fallback_fmt: LatexCounterFormat


class LatexSetup(RenderSetup[LatexRenderer]):
    standalone: bool
    """Is the document standalone (has preamble and \\begin{document}) or not"""

    document_class: MaybeUnset[str]
    """What is the \\documentclass (must be set by a plugin through require_document_class() if standalone"""
    declared_latex_counters: Dict[str, LatexCounterDecl]
    """The non-magic LaTeX counters that have been declared"""
    latex_counter_override_fmt: Dict[str, LatexCounterFormat]
    """Overridden formatting for non-magic LaTeX counters. Backref methods must configure themselves to use these."""
    latex_counter_backref_method: Dict[str, Optional[LatexBackrefMethod]]
    """The backref method for each non-magic LaTeX counters. Will always be a key to backref_impls."""

    shell_escape_reasons: List[str]
    """Reasons plugins called .request_shell_escape()"""
    requested_packages: Dict[str, LatexPackageRequirements]
    """Requested packages built up through calls to .request_latex_package()"""

    tt_counter_links: List[CounterLink]
    """Requested (parent, child) links in the turnip_text counter hierarchy"""

    # turnip_text:latex counters are (0 or 1):1 - LaTeX counters may be declared but not used
    tt_counter_to_latex_counter: Dict[str, str]
    latex_counter_to_tt_counter: Dict[str, str]
    # Map turnip_text to LaTeX counters for "magic" counters that LaTeX steps by itself in a way turnip_text cannot replicate
    magic_tt_counter_to_latex_counter: Dict[str, str]

    backref_impls: Dict[LatexBackrefMethod, LatexBackrefMethodImpl]

    emitter: EmitterDispatch[LatexRenderer]

    # These are initialized at the end of __init__ after the plugins have set up the counter links and turnip_text:latex mappings
    tt_counters: CounterState
    latex_counter_hierarchy: CounterHierarchy
    latex_counter_to_parent: Dict[str, Optional[str]]

    def __init__(
        self,
        plugins: Iterable[RenderPlugin[LatexRenderer, "LatexSetup"]],
        standalone: bool = False,
        counter_link_override: Optional[Iterable[CounterLink]] = None,
        latex_counter_format_override: Optional[Dict[str, LatexCounterFormat]] = None,
        # TODO config for the backref methods
    ) -> None:
        super().__init__(plugins)

        self.standalone = standalone

        self.document_class = UNSET
        self.declared_latex_counters = {}
        if latex_counter_format_override:
            self.latex_counter_override_fmt = latex_counter_format_override
        else:
            self.latex_counter_override_fmt = {}
        self.latex_counter_backref_method = {}

        self.shell_escape_reasons = []
        self.requested_packages = {}

        if counter_link_override:
            self.tt_counter_links = list(counter_link_override)
        else:
            self.tt_counter_links = []
        self.tt_counter_to_latex_counter = {}
        self.latex_counter_to_tt_counter = {}
        self.magic_tt_counter_to_latex_counter = {}

        self.backref_impls = {
            # TODO make loading cleveref optional
            LatexBackrefMethod.Cleveref: LatexCleveref(),
            LatexBackrefMethod.Hyperlink: LatexHyperlink(),
            LatexBackrefMethod.PageRef: LatexPageRef(),
        }

        self.emitter = LatexRenderer.default_emitter_dispatch()

        # TODO should only require these if used!
        self.request_latex_package(
            "hyperref", "backrefs using hypertarget+hyperref etc."
        )
        self.request_latex_package("cleveref", "backrefs using cleveref")

        # This allows plugins to register with the emitter and request specific counter links, packages, declare tt counters, etc.
        for p in plugins:
            p._register(self)

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
        self.latex_counter_hierarchy = build_counter_hierarchy(
            full_latex_counter_links, set(self.declared_latex_counters.keys())
        )
        tt_counter_hierarchy = map_counter_hierarchy(
            self.latex_counter_hierarchy,
            lambda latex_counter: self.latex_counter_to_tt_counter.get(latex_counter),
        )
        # Add the magic turnip_text counters to the tt_hierarchy - LaTeX doesn't consider them, but internally we still need to be able to step them
        for magic_tt_counter in self.magic_tt_counter_to_latex_counter:
            tt_counter_hierarchy[magic_tt_counter] = {}
        self.tt_counters = CounterState(tt_counter_hierarchy)

        # We can also build a hierarchy of LaTeX counters, which we use to generate the final point-to-point relations
        # TODO don't do the work of resolving the counter links twice :(
        self.latex_counter_to_parent, _ = resolve_counter_links(
            full_latex_counter_links,
            set(self.latex_counter_to_tt_counter.keys()),
        )

    def require_document_class(self, document_class: str) -> None:
        if self.document_class is not UNSET and self.document_class != document_class:
            raise RuntimeError(
                f"Conflicting document_class requirements: '{self.document_class}' and '{document_class}'"
            )
        self.document_class = document_class

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

    def request_shell_escape(self, reason: str) -> None:
        self.shell_escape_reasons.append(reason)

    def request_latex_package(
        self, package: str, reason: str, options: Iterable[str] | None = None
    ) -> None:
        package_obj = self.requested_packages.get(package, None)
        if package_obj is None:
            # Wasn't in the dict
            self.requested_packages[package] = LatexPackageRequirements(
                package=package,
                reasons=[reason],
                options=set(options) if options else set(),
            )
        else:
            package_obj.reasons.append(reason)
            # TODO check conflicts between options
            if options:
                package_obj.options.update(options)

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
        self, counter: str, parent_counter: Optional[str]
    ) -> None:
        # Apply the requested counter links
        self.tt_counter_links.append((parent_counter, counter))

    def gen_dfs_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]]:
        vs: List[Tuple[VisitorFilter, VisitorFunc]] = [
            (None, self.tt_counters.count_anchor_if_present)
        ]
        for p in self.plugins:
            v = p._make_visitors()
            if v:
                vs.extend(v)
        return vs

    def known_node_types(
        self,
    ) -> Iterable[type[Block] | type[Inline] | type[Header]]:
        return self.emitter.renderer_keys()

    def known_countables(self) -> Iterable[str]:
        return self.tt_counters.anchor_kind_to_parent_chain.keys()

    def register_file_generator_jobs(
        self,
        doc_setup: DocSetup,
        document: Document,
        output_file_name: Optional[str],
    ) -> None:
        if self.document_class is UNSET:
            if self.standalone:
                raise RuntimeError("Document class was not declared by any plugin!")
            else:
                document_class = None
        else:
            document_class = cast(str, self.document_class)

        tt_counter_to_spec = {}
        latex_counter_to_spec = {}
        for latex_counter in counter_hierarchy_dfs(self.latex_counter_hierarchy):
            decl = self.declared_latex_counters[latex_counter]
            backref_method = self.latex_counter_backref_method[latex_counter]
            tt_counter = self.latex_counter_to_tt_counter.get(latex_counter)

            spec = LatexCounterSpec(
                tt_counter=tt_counter,
                latex_counter=latex_counter,
                backref_impl=(
                    self.backref_impls[backref_method]
                    if backref_method is not None
                    else None
                ),
                provided_by_docclass_or_package=decl.provided_by_docclass_or_package,
                default_reset_latex_counter=decl.default_reset_latex_counter,
                reset_latex_counter=self.latex_counter_to_parent[latex_counter],
                fallback_fmt=decl.fallback_fmt,
                override_fmt=self.latex_counter_override_fmt.get(latex_counter),
            )
            if tt_counter:
                tt_counter_to_spec[tt_counter] = spec
            latex_counter_to_spec[latex_counter] = spec

        requirements = LatexRequirements(
            document_class,
            shell_escape=self.shell_escape_reasons,
            packages=self.requested_packages,
            tt_counter_to_latex=tt_counter_to_spec,
            latex_counter_to_latex=latex_counter_to_spec,
            magic_tt_counters=self.magic_tt_counter_to_latex_counter,
        )

        # Make a render job and register it in the build system.
        def render_job(_ins: Dict[str, JobInputFile], out: JobOutputFile) -> None:
            with out.open_write_text() as write_to:
                renderer = LatexRenderer(
                    doc_setup,
                    requirements,
                    self.tt_counters,
                    self.emitter,
                    write_to,
                )
                renderer.emit_document(document)

        doc_setup.build_sys.register_file_generator(
            render_job,
            inputs={},
            output_relative_path=output_file_name or "document.tex",
        )
