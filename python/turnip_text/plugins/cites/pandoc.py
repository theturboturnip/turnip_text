import json
from typing import Any

import turnip_text.render.pandoc.pandoc_types as pan
from turnip_text.build_system import BuildSystem, JobOutputFile, ProjectRelativePath
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.cites import (
    Bibliography,
    Citation,
    CitationEnvPlugin,
    CiteAuthor,
)
from turnip_text.plugins.cites.markdown import LATEXLIKE_CSL
from turnip_text.render.pandoc import (
    PandocPlugin,
    PandocRenderer,
    PandocSetup,
    map_json_to_pan_metavalue,
)


class PandocCitationPlugin(PandocPlugin, CitationEnvPlugin):
    _csl_json_path: ProjectRelativePath
    _citationNoteNum: int

    def __init__(self, csl_json_path: ProjectRelativePath):
        self._csl_json_path = csl_json_path
        self._citationNoteNum = 0

    def _register(self, build_sys: BuildSystem, setup: PandocSetup) -> None:
        super()._register(build_sys, setup)
        # Embed the bibliography in the metadata
        with build_sys.resolve_input_file(self._csl_json_path).open_read_text() as f:
            csl_json = json.load(f)
        assert isinstance(csl_json, list), "CSL JSON must be a JSON-encoded list"
        setup.meta[0]["references"] = map_json_to_pan_metavalue(csl_json)
        # Enable citation processing
        setup.add_pandoc_options("--citeproc")

        # Set a CSL style.
        # This requires a real CSL file
        # TODO need to refactor the build system to make this possible
        # def write_real_output_csl(inputs: Any, output: JobOutputFile) -> None:
        #     if not output.external_path:
        #         print(
        #             "Pandoc CSL needs an external CSL path, but the build system didn't provide one. pandoc will fail."
        #         )
        #     with open(output.external_path, "w") as f:
        #         f.write(LATEXLIKE_CSL)

        # output_csl_path = build_sys.register_file_generator(
        #     write_real_output_csl, {}, "/temp/citation.csl"
        # )
        # setup.add_pandoc_options(f"--csl={output_csl_path}")

        setup.makers.register_inline(Citation, self._make_cite)
        setup.makers.register_inline(CiteAuthor, self._make_citeauthor)
        setup.makers.register_block(
            # "it will be placed in a div with id refs, if one exists:"
            # https://pandoc.org/MANUAL.html#placement-of-the-bibliography
            Bibliography,
            lambda bib, renderer, fmt: pan.Div(("refs", [], []), []),
        )

    def _make_cite(
        self, citation: Citation, renderer: PandocRenderer, fmt: FmtEnv
    ) -> pan.Inline:
        cites = []
        for citekey in citation.citekeys:
            self._citationNoteNum += 1
            cites.append(
                pan.Citation(
                    citekey,
                    [],  # No prefix
                    [],  # No suffix (for now)
                    pan.NormalCitation(),
                    self._citationNoteNum,  # The number of this citation in the list of (citation,citeauthor)s
                    0,  # Hash - this can be 0?
                )
            )
        # Put the citenote as the suffix of the last citation
        if citation.citenote:
            cites[-1][2] = renderer.make_inline_scope_list(citation.citenote)
        return pan.Cite(
            cites, [pan.Str(f"@{citekey}") for citekey in citation.citekeys]
        )

    def _make_citeauthor(
        self, citation: CiteAuthor, renderer: PandocRenderer, fmt: FmtEnv
    ) -> pan.Inline:
        self._citationNoteNum += 1
        return pan.Cite(
            [
                pan.Citation(
                    citation.citekey,
                    [],  # No prefix
                    [],  # No suffix
                    pan.AuthorInText(),  # This is just a citation putting the author in the text
                    self._citationNoteNum,  # The number of this citation in the list of (citation,citeauthor)s
                    0,  # Hash - this can be 0?
                )
            ],
            [pan.Str(f"@{citation.citekey}")],
        )
