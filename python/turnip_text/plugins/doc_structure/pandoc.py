from typing import List, Literal

import turnip_text.render.pandoc.pandoc_types as pan
from turnip_text import Text
from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.doc_structure import (
    AppendixHeader,
    BasicHeader,
    StructureEnvPlugin,
    TableOfContents,
    TitleBlock,
)
from turnip_text.render.manual_numbering import SimpleCounterFormat, SimpleCounterStyle
from turnip_text.render.pandoc import (
    PandocPlugin,
    PandocRenderer,
    PandocSetup,
    null_attr,
)

HEADER_NAMES = [
    "chapter",
    "section",
    "subsection",
    "subsubsection",
    "paragraph",
    "subparagraph",
]


class PandocStructurePlugin(PandocPlugin, StructureEnvPlugin):
    # 0-indexed list of formats e.g. h1 uses self.header_fmts[0]
    _header_fmts: List[SimpleCounterFormat[SimpleCounterStyle]]

    def __init__(self, h1: Literal["chapter"] | Literal["section"] = "section", add_title: bool=True, add_toc: bool=True) -> None:
        super().__init__(add_title, add_toc)
        self._header_fmts = [
            SimpleCounterFormat(
                header_name,
                style=SimpleCounterStyle.Arabic,
            )
            for header_name in HEADER_NAMES[HEADER_NAMES.index(h1) :]
        ]

    def _register(self, build_sys: BuildSystem, setup: PandocSetup) -> None:
        super()._register(build_sys, setup)

        # TitleBlock and TableOfContents are automatically created by Pandoc when requested
        # These callbacks only set up the metadata and command line args necessary to do so
        setup.makers.register_block(TitleBlock, self._handle_title_block)
        setup.makers.register_block(TableOfContents, self._handle_toc)

        setup.makers.register_header(BasicHeader, self._make_header)
        setup.makers.register_header(AppendixHeader, self._make_appendix_header)

        for i, fmt in enumerate(self._header_fmts):
            setup.define_renderable_counter(f"h{i+1}", fmt)
            if i > 0:
                setup.request_counter_parent(f"h{i+1}", f"h{i}")
        setup.define_renderable_counter(
            "appendix",
            SimpleCounterFormat(
                name="appendix",
                style=SimpleCounterStyle.AlphUpper,
            ),
        )

    def _handle_title_block(
        self, title_block: TitleBlock, renderer: PandocRenderer, fmt: FmtEnv
    ) -> pan.Div:
        renderer.pandoc_options.append("--standalone")

        if title_block.metadata.title:
            renderer.meta[0]["title"] = pan.MetaInlines(
                [renderer.make_inline(title_block.metadata.title)]
            )
        if title_block.metadata.subtitle:
            renderer.meta[0]["subtitle"] = pan.MetaInlines(
                [renderer.make_inline(title_block.metadata.subtitle)]
            )
        renderer.meta[0]["authors"] = pan.MetaList(
            [
                pan.MetaMap({"name": pan.MetaInlines([renderer.make_inline(author)])})
                for author in title_block.metadata.authors
            ]
        )
        return pan.Div(null_attr(), [])

    def _handle_toc(
        self, toc: TableOfContents, renderer: PandocRenderer, fmt: FmtEnv
    ) -> pan.Div:
        renderer.pandoc_options.append(
            "--toc"
        )  # Some documentation says --toc=true is allowed, but not this embedded version of pandoc...
        renderer.pandoc_options.append(f"--toc-depth={toc.depth}")
        return pan.Div(null_attr(), [])

    def _make_header(
        self,
        header: BasicHeader,
        renderer: PandocRenderer,
        fmt: FmtEnv,
    ) -> pan.Header:
        # TODO - if in e.g. docx warn people to use numbered-sections-template.docx instead of numbering?
        attr = renderer.make_anchor_attr(header.anchor)
        title = renderer.make_inline_scope_list(header.title)
        if header.anchor:
            # Do the numbering: compute the number, put "{number} [emdash] {title}"
            title = (
                renderer.make_text_inline_list(
                    Text(
                        renderer.anchor_to_number_text(header.anchor).text + " \u2014 "
                    )
                )
                + title
            )
        return pan.Header(header.weight, attr, title)

    def _make_appendix_header(
        self,
        header: AppendixHeader,
        renderer: PandocRenderer,
        fmt: FmtEnv,
    ) -> pan.Header:
        attr = renderer.make_anchor_attr(header.anchor)
        # Do the numbering: compute the number, put "Appendix {number} [emdash] {title}"
        title = renderer.make_text_inline_list(
            Text(renderer.anchor_to_ref_text(header.anchor).text + " \u2014 ")
        ) + renderer.make_inline_scope_list(header.title)
        return pan.Header(header.weight, attr, title)
