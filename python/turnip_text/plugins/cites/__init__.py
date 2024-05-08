from dataclasses import dataclass
from typing import Sequence, Set

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


# Moons ago I considered replacing this with Backref. This should not be replaced with Backref,
# because it has specific context in how it is rendered. Renderer plugins may mutate the document
# and replace these with Backrefs if they so choose.
@dataclass(frozen=True)
class Citation(UserNode, Inline, InlineScopeBuilder):
    citenote: InlineScope | None
    citekeys: Set[str]
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

    def _mutate_document(
        self, doc_env: DocEnv, fmt: FmtEnv, toplevel: Document
    ) -> Document:
        if not self._has_bib:
            toplevel.push_segment(
                DocSegment(
                    doc_env.heading1(num=False) @ "Bibliography",
                    BlockScope([Bibliography()]),
                    [],
                )
            )
        return toplevel

    @in_doc
    def cite(self, doc_env: DocEnv, *citekeys: str) -> Inline:
        citekey_set: set[str] = set(citekeys)
        for c in citekey_set:
            if not isinstance(c, str):
                raise ValueError(f"Inappropriate citation key: {c}. Must be a string")
        self._has_citations = True
        return Citation(citekeys=citekey_set, citenote=None)

    @pure_fmt
    def citeauthor(self, fmt: FmtEnv, citekey: str) -> Inline:
        return CiteAuthor(citekey)

    @in_doc
    def bibliography(self, doc_env: DocEnv) -> Bibliography:
        self._has_bib = True
        return Bibliography()
