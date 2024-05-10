from typing import Callable, Dict, Iterable, List, Optional, Tuple, cast

from typing_extensions import override

from turnip_text import Block, Document, Header, Inline
from turnip_text.build_system import BuildSystem, JobInputFile, JobOutputFile
from turnip_text.doc.dfs import VisitorFilter, VisitorFunc
from turnip_text.env_plugins import FmtEnv
from turnip_text.helpers import UNSET, MaybeUnset
from turnip_text.plugins.anchors import StdAnchorPlugin
from turnip_text.render import EmitterDispatch, RenderPlugin, RenderSetup
from turnip_text.render.counters import CounterLink
from turnip_text.render.latex.backrefs import LatexBackrefMethod
from turnip_text.render.latex.counter_resolver import (
    LatexCounterResolver,
    ResolvedTTAndLatexCounters,
)
from turnip_text.render.latex.package_resolver import LatexPackageResolver
from turnip_text.render.latex.renderer import (
    LatexCounterFormat,
    LatexRenderer,
    LatexRequirements,
)

LatexPlugin = RenderPlugin["LatexSetup"]


class LatexSetup(RenderSetup[LatexRenderer]):
    standalone: bool
    """Is the document standalone (has preamble and \\begin{document}) or not"""

    document_class: MaybeUnset[str]
    """What is the \\documentclass (must be set by a plugin through require_document_class() if standalone"""

    package_resolver: LatexPackageResolver

    counter_resolver: LatexCounterResolver
    resolved_counters: Optional[ResolvedTTAndLatexCounters]

    preamble_callbacks: List[Callable[[LatexRenderer], None]]
    """Callbacks registered by plugins to emit things in the preamble."""

    emitter: EmitterDispatch[LatexRenderer]

    def __init__(
        self,
        standalone: bool = False,
        counter_link_override: Optional[Iterable[CounterLink]] = None,
        # TODO make this be in terms of tt_counter so the whole thing is tt_counters?
        latex_counter_format_override: Optional[Dict[str, LatexCounterFormat]] = None,
        legal_backref_methods: Optional[List[LatexBackrefMethod]] = None,
        # TODO config for the backref methods
    ) -> None:
        super().__init__()

        self.standalone = standalone

        self.document_class = UNSET

        self.package_resolver = LatexPackageResolver()
        # Default packages
        self.package_resolver.request_latex_package(
            "fontenc", reason="allows a wider array of text characters", options=["T1"]
        )
        self.package_resolver.request_latex_package(
            "lmodern", reason="basic standard font for T1 text encoding"
        )

        self.counter_resolver = LatexCounterResolver(
            counter_link_override, latex_counter_format_override, legal_backref_methods
        )
        self.resolved_counters = None

        self.preamble_callbacks = []

        self.emitter = LatexRenderer.default_emitter_dispatch()

    @override
    def register_plugins(
        self,
        build_sys: BuildSystem,
        plugins: Iterable["RenderPlugin[LatexSetup]"],
    ) -> None:
        # This allows plugins to register with the emitter and request specific counter links, packages, declare tt counters, etc.
        super().register_plugins(build_sys, plugins)
        self.resolved_counters = self.counter_resolver.resolve_all()
        # Don't try to use the counter_resolve anymore.
        self.counter_resolver = None  # type:ignore

    def require_document_class(self, document_class: str) -> None:
        if self.document_class is not UNSET and self.document_class != document_class:
            raise RuntimeError(
                f"Conflicting document_class requirements: '{self.document_class}' and '{document_class}'"
            )
        self.document_class = document_class

    def add_preamble_section(self, callback: Callable[[LatexRenderer], None]) -> None:
        self.preamble_callbacks.append(callback)

    def gen_dfs_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]]:
        resolved_counters = self.resolved_counters
        # resolved_counters stops being None once we call register_plugins
        assert (
            resolved_counters
        ), "Can't call gen_dfs_visitors until after you call register_plugins"

        vs: List[Tuple[VisitorFilter, VisitorFunc]] = [
            (None, resolved_counters.tt_counters.count_anchor_if_present)
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
        resolved_counters = self.resolved_counters
        # resolved_counters stops being None once we call register_plugins
        assert (
            resolved_counters
        ), "Can't call known_countables until after you call register_plugins"

        return resolved_counters.tt_counters.anchor_kind_to_parent_chain.keys()

    def register_file_generator_jobs(
        self,
        fmt: FmtEnv,
        anchors: StdAnchorPlugin,
        document: Document,
        build_sys: BuildSystem,
        output_file_name: Optional[str],
    ) -> None:
        if self.standalone:
            document_class = None
        else:
            if self.document_class is UNSET:
                raise RuntimeError("Document class was not declared by any plugin!")
            document_class = cast(str, self.document_class)

        resolved_counters = self.resolved_counters
        # resolved_counters stops being None once we call register_plugins
        assert (
            resolved_counters
        ), "Can't call register_file_generator_jobs until after you call register_plugins"

        tt_counter_to_spec = {
            counter.tt_counter: counter
            for counter in resolved_counters.latex_renderable_counters
            if counter.tt_counter
        }
        latex_counter_to_spec = {
            counter.latex_counter: counter
            for counter in resolved_counters.latex_renderable_counters
        }

        for backref_impl in resolved_counters.backref_impls:
            backref_impl.request_packages(self.package_resolver)

        resolved_packages = self.package_resolver.resolve_all()

        requirements = LatexRequirements(
            document_class,
            shell_escape=resolved_packages.shell_escape_reasons,
            packages=resolved_packages.packages,
            preamble_callbacks=self.preamble_callbacks,
            tt_counter_to_latex=tt_counter_to_spec,
            latex_counter_to_latex=latex_counter_to_spec,
            magic_tt_counter_to_latex_counter=resolved_counters.magic_tt_counter_to_latex_counter,
        )

        # Make a render job and register it in the build system.
        def render_job(_ins: Dict[str, JobInputFile], out: JobOutputFile) -> None:
            with out.open_write_text() as write_to:
                renderer = LatexRenderer(
                    fmt,
                    anchors,
                    requirements,
                    resolved_counters.tt_counters,
                    self.emitter,
                    write_to,
                )
                renderer.emit_document(document)

        build_sys.register_file_generator(
            render_job,
            inputs={},
            output_relative_path=output_file_name or "document.tex",
        )
