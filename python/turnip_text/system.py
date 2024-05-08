"""The phases of parsing and creating a new document:

1. Parsing
   This requires creating a set of turnip_text.doc.DocPlugin, which define the interface used by inline code inside the document.
   The document may also have metadata which can be retrieved at this point.
   Python creates a DocSetup instance with the relevant plugins and metadata, points it at TurnipTextSource.
   This creates a Document and a mapping of [Anchor, Block] called "floating space". Floating space includes
   e.g. footnotes or figures which may have their definitions "float" from their point-of-definition to different places in the text stream.
2. Mutating
   The document plugins and language-specific renderer plugins may both want to inject new state into the document once the user has completed it.
   e.g. Collating all citations created inside the document by the user and making a secondary list of them.
   e.g. Creating Bibliography section at the end of the document?
   This phase must also pull all items out of "floating space" and insert them somewhere definitive inside the document.
   TODO i haven't decided whether it's sensible to allow plugins to put more things in floating space and hope someone else pulls them out - my instinct is probably not.
   TODO the current system isn't that they actually get shoved back into the document - just that some things are "portals" into floating space for the purposes of visiting and counting.
   The DocSetup is taken as input, alongside a RenderSetup which collates the renderer plugins.
   This phase creates a final Document tree which is considered "frozen" for the rest of the phase.
3. Visiting and Counting
   Once the document is frozen, the RendererPlugins may want to gather information from them:
   e.g. a citation plugin may need to gather the list of cited works
   e.g. ...?
   and certain language backends will need to count important items like sections, figures, etc.
   This is exposed as a single DFS traversal of the tree which calls different visitor functions
   (provided by renderer plugins or the renderersetup) based on the type of the traversed nodes.
   The RenderSetup provides the list of visitor functions, generated from the set of plugins and whatever counter system the renderer uses.
   The RenderSetup must already have the hierarchy of which counters are reset by which other counters.
   This phase mutates the internal state of the renderer plugins and RenderSetup.
4. Rendering
   Finally, the RenderSetup is converted into a single-use Renderer which outputs to a file/StringIO.
   The Renderer iterates through the frozen document, emitting the elements by calling into RendererPlugin-defined functions.
   This mutates internal RendererPlugin state, may take info from the RenderSetup e.g. resolved LaTeX package information, consumes the document, and mutates a maybe-passed-in IO handle."""

from typing import List, Optional, Sequence, Set, Type, Union

from turnip_text import Block, Header, Inline, parse_file
from turnip_text.build_system import (
    BuildSystem,
    OutputRelativePath,
    ProjectRelativePath,
)
from turnip_text.doc.dfs import DocumentDfsPass
from turnip_text.env_plugins import EnvPlugin
from turnip_text.plugins.anchors import StdAnchorPlugin
from turnip_text.render import RenderPlugin, TRenderSetup


def parse_and_emit(
    build_sys: BuildSystem,
    src_path: ProjectRelativePath,
    out_path: Optional[OutputRelativePath],
    render_setup: TRenderSetup,
    plugins: Sequence[RenderPlugin[TRenderSetup]],
) -> None:
    # Phase 0 - Setup plugins, contexts, and initialize the render setup
    anchors = StdAnchorPlugin()
    plugins_with_anchors: List[EnvPlugin] = list(plugins)
    plugins_with_anchors.append(anchors)
    fmt, doc_env = EnvPlugin._make_contexts(build_sys, plugins_with_anchors)

    render_setup.register_plugins(build_sys, plugins)

    # Phase 1 - Parsing
    src = build_sys.resolve_turnip_text_source(src_path)
    document = parse_file(src, doc_env.__dict__)

    # Phase 2 - Mutation
    exported_nodes: Set[Type[Union[Block, Inline, Header]]] = set()
    exported_countables: Set[str] = set()

    for plugin in plugins_with_anchors:
        exported_nodes.update(plugin._doc_nodes())
        exported_countables.update(plugin._countables())
        # TODO we need to handle mutations differently
        document = plugin._mutate_document(doc_env, fmt, document)

    # Now freeze the document so other code can't mutate it
    doc_env._frozen = True

    # TODO right now the document parsing process uses portals instead of actually expecting the mutation phase to pull things out of floating-space. Once that changes, re-enable this check.
    # if doc_setup.anchors._anchored_floats:
    #     raise RuntimeError(
    #         f"After document mutation there were still blocks left in floating space: {doc_setup.anchors._anchored_floats.keys()}.\nThese blocks will not be processed or put in the final document."
    #     )

    # Check that all the nodes used in the document are handled by renderer setup and can be emitted
    missing_renderers = exported_nodes.difference(render_setup.known_node_types())
    if missing_renderers:
        raise RuntimeError(
            f"Some node types were not given renderers by any plugin, but are used by the document: {missing_renderers}"
        )

    # Check that all the countables in the document are known by the renderer setup and can be counted
    missing_doc_counters = exported_countables.difference(
        render_setup.known_countables()
    )
    if missing_doc_counters:
        raise RuntimeError(
            f"Some counters are not handled by the RenderSetup, but are used by the document: {missing_doc_counters}"
        )

    # Phase 3 - Visiting and Counting
    DocumentDfsPass(render_setup.gen_dfs_visitors()).dfs_over_document(
        document,
        anchors,
    )

    # Phase 4 - Rendering
    # Create the main document render jobs
    render_setup.register_file_generator_jobs(
        fmt, anchors, document, build_sys, out_path
    )
    # Run all the jobs accumulated in the build system.
    build_sys.run_jobs()
