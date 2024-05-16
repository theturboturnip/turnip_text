from dataclasses import dataclass
from typing import Sequence, Set

from turnip_text import (
    Block,
    Blocks,
    DocSegment,
    Document,
    Header,
    Inline,
    Inlines,
    InlinesBuilder,
)
from turnip_text.doc.user_nodes import UserNode
from turnip_text.env_plugins import DocEnv, EnvPlugin, FmtEnv, in_doc, pure_fmt
from turnip_text.helpers import UserInlineScopeBuilder
from typing_extensions import override


# Moons ago I considered replacing this with Backref. This should not be replaced with Backref,
# because it has specific context in how it is rendered. Renderer plugins may mutate the document
# and replace these with Backrefs if they so choose.
@dataclass
class Citation(UserNode, Inline, UserInlineScopeBuilder):
    citenote: Inlines | None
    citekeys: Set[str]
    anchor = None

    @override
    def child_nodes(self) -> Inlines | None:
        return self.citenote

    def build_from_inlines(self, inlines: Inlines) -> Inline:
        return Citation(citekeys=self.citekeys, citenote=inlines)


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
        if not self._has_bib:
            doc.append_header(
                doc_env.h1(num=False) @ "Bibliography"
            ).contents.append_block(Bibliography())

    def cite(self, *citekeys: str) -> Inline:
        if not citekeys:
            raise ValueError("Must provide at least one citekey to cite()")
        citekey_set: set[str] = set(citekeys)
        for c in citekey_set:
            if not isinstance(c, str):
                raise ValueError(f"Inappropriate citation key: {c}. Must be a string")
        return Citation(citekeys=citekey_set, citenote=None)

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
