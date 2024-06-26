from dataclasses import dataclass
from typing import List, Sequence, Set

from typing_extensions import override

from turnip_text import (
    Block,
    BlockScope,
    DocSegment,
    Document,
    Header,
    Inline,
    InlineScope,
    InlineScopeBuilder,
)
from turnip_text.doc.user_nodes import UserNode
from turnip_text.env_plugins import DocEnv, EnvPlugin, FmtEnv, in_doc, pure_fmt
from turnip_text.helpers import UserInlineScopeBuilder


# Moons ago I considered replacing this with Backref. This should not be replaced with Backref,
# because it has specific context in how it is rendered. Renderer plugins may mutate the document
# and replace these with Backrefs if they so choose.
@dataclass
class Citation(UserNode, Inline, UserInlineScopeBuilder):
    citenote: InlineScope | None
    citekeys: List[str]
    anchor = None

    @override
    def child_nodes(self) -> InlineScope | None:
        return self.citenote

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        return Citation(citekeys=self.citekeys, citenote=inls)


@dataclass(frozen=True)
class CiteAuthor(Inline):
    citekey: str


class Bibliography(Block):
    pass


class CitationEnvPlugin(EnvPlugin):
    _has_citations: bool = False
    _has_bib: bool = False

    def _doc_nodes(
        self,
    ) -> Sequence[type[Block] | type[Inline] | type[Header]]:
        return (
            Citation,
            CiteAuthor,
            Bibliography,
        )

    def _mutate_document(self, doc_env: DocEnv, fmt: FmtEnv, doc: Document) -> None:
        super()._mutate_document(doc_env, fmt, doc)
        # TODO better doc DFS walking to find Bibliography
        # TODO only add the bibliography if walking the doc finds Citation
        if not self._has_bib:
            doc.append_header(
                doc_env.h1(num=False) @ "Bibliography"
            ).contents.append_block(Bibliography())

    def cite(self, *citekeys: str) -> Inline:
        if not citekeys:
            raise ValueError("Must provide at least one citekey to cite()")
        for c in citekeys:
            if not isinstance(c, str):
                raise ValueError(f"Inappropriate citation key: {c}. Must be a string")
        return Citation(citenote=None, citekeys=list(citekeys))

    def citeauthor(self, citekey: str) -> Inline:
        return CiteAuthor(citekey)

    @in_doc
    def bibliography(self, doc_env: DocEnv) -> Bibliography:
        self._has_bib = True
        return Bibliography()

    # If a citation happens inside a Raw emitted without going through this plugin,
    # you can register that fact here.
    def register_raw_cite(self, *citekeys: str) -> None:
        pass
