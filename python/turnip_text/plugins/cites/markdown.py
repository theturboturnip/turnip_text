import io
import json
from dataclasses import dataclass
from typing import Generator, List, Optional, Set, Tuple

import citeproc  # type: ignore

from turnip_text import Text
from turnip_text.build_system import BuildSystem, ProjectRelativePath
from turnip_text.doc.dfs import VisitorFilter, VisitorFunc
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.cites import (
    Bibliography,
    Citation,
    CitationEnvPlugin,
    CiteAuthor,
)
from turnip_text.render.markdown.renderer import (
    MarkdownPlugin,
    MarkdownRenderer,
    MarkdownSetup,
)


class MarkdownCitationPlugin_UncheckedBib(MarkdownPlugin, CitationEnvPlugin):
    _ordered_citations: List[str]
    _referenced_citations: Set[str]

    def __init__(self) -> None:
        super().__init__()
        self._ordered_citations = []
        self._referenced_citations = set()

    def _register(self, build_sys: BuildSystem, setup: MarkdownSetup) -> None:
        setup.emitter.register_block_or_inline(Citation, self._emit_cite)
        setup.emitter.register_block_or_inline(CiteAuthor, self._emit_citeauthor)
        setup.emitter.register_block_or_inline(Bibliography, self._emit_bibliography)

    def _register_citation(self, citekey: str) -> None:
        if citekey not in self._referenced_citations:
            self._referenced_citations.add(citekey)
            self._ordered_citations.append(citekey)

    def _make_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]] | None:
        def regsiter_many_citations(c: Citation) -> None:
            for k in c.citekeys:
                self._register_citation(k)

        return [
            (Citation, regsiter_many_citations),
            (CiteAuthor, lambda ca: self._register_citation(ca.citekey)),
        ]

    # TODO make Citations use backrefs? Requires document mutations which we don't have yet.

    def _emit_cite(
        self, cite: Citation, renderer: MarkdownRenderer, fmt: FmtEnv
    ) -> None:
        # TODO what happens with unmarkdownable labels? e.g. labels with backslash or something. need to check that when loading.
        # TODO also maybe people wouldn't want those labels being exposed?

        if cite.citenote:
            renderer.emit(Text("("))
        for citekey in cite.citekeys:
            renderer.emit(fmt.url(f"#{citekey}") @ f"[{citekey}]")
        if cite.citenote:
            renderer.emit(Text(", "), cite.citenote, Text(")"))

    def _emit_citeauthor(
        self,
        citeauthor: CiteAuthor,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        renderer.emit(Text("The authors of "))
        renderer.emit(fmt.url(f"#{citeauthor.citekey}") @ f"[{citeauthor.citekey}]")

    def _emit_bibliography(
        self,
        bib: Bibliography,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        # TODO actual reference rendering!
        def bib_gen() -> Generator[None, None, None]:
            for citekey in self._referenced_citations:
                renderer.emit_empty_tag("a", f'id="{citekey}"')
                renderer.emit(
                    Text(f"[{citekey}]: TODO make citation text for {citekey}"),
                )
                yield

        renderer.emit_join_gen(bib_gen(), renderer.emit_break_paragraph)


# If this were in a file it might benefit from <?xml version="1.0" encoding="utf-8"?>,
# but the XML importer for citeproc complains when we pass it an explicitly unicode string with that tag.
LATEXLIKE_CSL = """<?xml version="1.0"?>
<style xmlns="http://purl.org/net/xbiblio/csl" class="in-text" version="1.0" demote-non-dropping-particle="sort-only" default-locale="en-US">
  <info>
    <title>ACM SIG Proceedings ("et al." for 3+ authors)</title>
    <id>http://www.zotero.org/styles/acm-sig-proceedings</id>
    <link href="http://www.zotero.org/styles/acm-sig-proceedings" rel="self"/>
    <link href="http://www.acm.org/sigs/publications/proceedings-templates" rel="documentation"/>
    <author>
      <name>Naeem Esfahani</name>
      <email>nesfaha2@gmu.edu</email>
      <uri>http://mason.gmu.edu/~nesfaha2/</uri>
    </author>
    <contributor>
      <name>Chris Horn</name>
      <email>chris.horn@securedecisions.com</email>
    </contributor>
    <contributor>
      <name>Patrick O'Brien</name>
    </contributor>
    <category citation-format="numeric"/>
    <category field="science"/>
    <category field="engineering"/>
    <updated>2017-07-15T11:28:14+00:00</updated>
    <rights license="http://creativecommons.org/licenses/by-sa/3.0/">This work is licensed under a Creative Commons Attribution-ShareAlike 3.0 License</rights>
  </info>
  <macro name="author">
    <choose>
      <if type="webpage">
        <text variable="title" suffix=":"/>
      </if>
      <else>
        <names variable="author">
          <name name-as-sort-order="all" and="text" sort-separator=", " initialize-with="." delimiter-precedes-last="never" delimiter=", "/>
          <label form="short" prefix=" "/>
          <substitute>
            <names variable="editor"/>
            <names variable="translator"/>
          </substitute>
        </names>
      </else>
    </choose>
  </macro>
  <macro name="editor">
    <names variable="editor">
      <name initialize-with="." delimiter=", " and="text"/>
      <label form="short" prefix=", "/>
    </names>
  </macro>
  <macro name="access">
    <choose>
      <if type="article-journal" match="any">
        <text variable="DOI" prefix=". DOI:https://doi.org/"/>
      </if>
    </choose>
  </macro>
  <citation collapse="citation-number">
    <sort>
      <key variable="citation-number"/>
    </sort>
    <layout prefix="[" suffix="]" delimiter=", ">
      <text variable="citation-number"/>
    </layout>
  </citation>
  <bibliography entry-spacing="0" second-field-align="flush" et-al-min="3" et-al-use-first="1">
    <sort>
      <key macro="author"/>
      <key variable="title"/>
    </sort>
    <layout suffix=".">
      <text variable="citation-number" prefix="[" suffix="] "/>
      <text macro="author" suffix=" "/>
      <date variable="issued" suffix=". ">
        <date-part name="year"/>
      </date>
      <choose>
        <if type="paper-conference">
          <group delimiter=". ">
            <text variable="title"/>
            <group delimiter=" ">
              <text variable="container-title" font-style="italic"/>
              <group delimiter=", ">
                <group delimiter=", " prefix="(" suffix=")">
                  <text variable="publisher-place"/>
                  <date variable="issued">
                    <date-part name="month" form="short" suffix=" "/>
                    <date-part name="year"/>
                  </date>
                </group>
                <text variable="page"/>
              </group>
            </group>
          </group>
        </if>
        <else-if type="article-journal">
          <group delimiter=". ">
            <text variable="title"/>
            <text variable="container-title" font-style="italic"/>
            <group delimiter=", ">
              <text variable="volume"/>
              <group delimiter=" ">
                <text variable="issue"/>
                <date variable="issued" prefix="(" suffix=")">
                  <date-part name="month" form="short" suffix=" "/>
                  <date-part name="year"/>
                </date>
              </group>
              <text variable="page"/>
            </group>
          </group>
        </else-if>
        <else-if type="patent">
          <group delimiter=". ">
            <text variable="title"/>
            <text variable="number"/>
            <date variable="issued">
              <date-part name="month" form="short" suffix=" "/>
              <date-part name="day" suffix=", "/>
              <date-part name="year"/>
            </date>
          </group>
        </else-if>
        <else-if type="thesis">
          <group delimiter=". ">
            <text variable="title" font-style="italic"/>
            <text variable="archive_location" prefix="Doctoral Thesis #"/>
            <text variable="publisher"/>
          </group>
        </else-if>
        <else-if type="report">
          <group delimiter=". ">
            <text variable="title" font-style="italic"/>
            <text variable="number" prefix="Technical Report #"/>
            <text variable="publisher"/>
          </group>
        </else-if>
        <else-if type="webpage">
          <group delimiter=". ">
            <text variable="URL" font-style="italic"/>
            <date variable="accessed" prefix="Accessed: ">
              <date-part name="year" suffix="-"/>
              <date-part name="month" form="numeric-leading-zeros" suffix="-"/>
              <date-part name="day" form="numeric-leading-zeros"/>
            </date>
          </group>
        </else-if>
        <else-if type="chapter paper-conference" match="any">
          <group delimiter=". ">
            <text variable="title"/>
            <text variable="container-title" font-style="italic"/>
            <text macro="editor"/>
            <text variable="publisher"/>
            <text variable="page"/>
          </group>
        </else-if>
        <else-if type="bill book graphic legal_case legislation motion_picture report song" match="any">
          <group delimiter=". ">
            <text variable="title" font-style="italic"/>
            <text variable="publisher"/>
          </group>
        </else-if>
        <else>
          <group delimiter=". ">
            <text variable="title"/>
            <text variable="container-title" font-style="italic"/>
            <text variable="publisher"/>
          </group>
        </else>
      </choose>
      <text macro="access"/>
    </layout>
  </bibliography>
</style>
"""


class MarkdownCiteProcCitationPlugin(MarkdownPlugin, CitationEnvPlugin):
    _bib_path: ProjectRelativePath
    _bib_is_csl_json: bool
    _csl_style_path: Optional[ProjectRelativePath]
    _bib: citeproc.CitationStylesBibliography

    def __init__(
        self,
        bibtex_path: Optional[ProjectRelativePath] = None,
        citeproc_json_path: Optional[ProjectRelativePath] = None,
        csl_path: Optional[ProjectRelativePath] = None,
    ) -> None:
        if bibtex_path and not citeproc_json_path:
            self._bib_path = bibtex_path
            self._bib_is_csl_json = False
        elif citeproc_json_path and not bibtex_path:
            self._bib_path = citeproc_json_path
            self._bib_is_csl_json = True
        elif citeproc_json_path and bibtex_path:
            raise ValueError(f"Can't use citeproc-json and bibtex at the same time")
        else:
            raise ValueError(
                f"Specify exactly one of (bibtex_path, citeproc_json_path)"
            )

        self._csl_style_path = csl_path

    def _register(self, build_sys: BuildSystem, setup: MarkdownSetup) -> None:
        bib_file = build_sys.resolve_input_file(self._bib_path)
        if self._bib_is_csl_json:
            with bib_file.open_read_text() as f:
                citeproc_json = json.loads(f.read())
            bib_source = citeproc.source.json.CiteProcJSON(citeproc_json)
        else:
            # TODO this sucks! citeproc doesn't support biblatex
            print(
                "Warning! BibTeX backend for citeproc-py doesn't support BibLaTeX constructs like @online"
            )
            with bib_file.open_read_text() as f:
                bib_source = citeproc.source.bibtex.BibTeX(f)

        # validation has some weird warnings that don't make much sense
        if self._csl_style_path:
            csl_style_file = build_sys.resolve_input_file(self._csl_style_path)
            bib_style = citeproc.CitationStylesStyle(
                csl_style_file.open_read_text(), validate=False
            )
        else:
            bib_style = citeproc.CitationStylesStyle(
                io.StringIO(LATEXLIKE_CSL), validate=False
            )

        # TODO need to implement a Markdown-friendly formatter
        self._bib = citeproc.CitationStylesBibliography(
            bib_style, bib_source, citeproc.formatter.html
        )

        # TODO need to manually sort once the document parse has finished?

        setup.emitter.register_block_or_inline(Citation, self._emit_citation)
        setup.emitter.register_block_or_inline(CiteAuthor, self._emit_citeauthor)
        setup.emitter.register_block_or_inline(Bibliography, self._emit_bibliography)

    def _make_visitors(self) -> List[Tuple[VisitorFilter, VisitorFunc]] | None:
        def foreach_citation(citation: Citation) -> None:
            citeproc_citation = citeproc.Citation(
                [citeproc.CitationItem(key) for key in citation.citekeys]
            )
            self._bib.register(citeproc_citation)
            citation.citeproc_cite = citeproc_citation  # type: ignore

        return [
            (Citation, foreach_citation),
        ]

    def _emit_citation(
        self,
        citation: Citation,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        # self._bib stores the keys as all lowercase
        anchor_target = f"#cite-{next(iter(citation.citekeys)).lower()}"
        citeproc_cite: citeproc.Citation = citation.citeproc_cite  # type:ignore
        renderer.emit(
            fmt.url(anchor_target)
            @ self._bib.cite(citeproc_cite, self._warn_invalid_citationitem)
        )

    @staticmethod
    def _warn_invalid_citationitem(item: citeproc.CitationItem) -> None:
        raise ValueError(
            f"Referenced key '{item.key}' that didn't exist in the bibliography"
        )

    def _emit_citeauthor(
        self,
        citeauthor: CiteAuthor,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        # citeauthor equivalent doesn't exist in CSL
        # See https://stackoverflow.com/a/65412491/4248422
        # TODO improve this
        renderer.emit(fmt.bold @ Text(f"<Author Of {citeauthor.citekey}>"))

    def _emit_bibliography(
        self,
        bibliography: Bibliography,
        renderer: MarkdownRenderer,
        fmt: FmtEnv,
    ) -> None:
        def bib_cites_gen() -> Generator[None, None, None]:
            for citekey, item in zip(self._bib.keys, self._bib.bibliography()):
                renderer.emit_empty_tag("a", f'id="{citekey}"')
                renderer.emit_raw(str(item))
                yield None

        # The citeproc formatter producess HTML
        with renderer.html_mode():
            renderer.emit_join_gen(
                bib_cites_gen(),
                renderer.emit_break_paragraph,
            )
