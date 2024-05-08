from dataclasses import dataclass
from typing import Iterable, Optional, Sequence

from typing_extensions import override

from turnip_text import Block, Inline, InlineScope, InlineScopeBuilder, Text
from turnip_text.doc.user_nodes import UserNode
from turnip_text.env_plugins import EnvPlugin, FmtEnv, pure_fmt


@dataclass(frozen=True)
class NamedUrl(UserNode, Inline, InlineScopeBuilder):
    name: Iterable[Inline] | None
    url: str
    anchor = None

    @override
    def child_nodes(self) -> Iterable[Inline] | None:
        return self.name

    def build_from_inlines(self, inls: InlineScope) -> Inline:
        return NamedUrl(url=self.url, name=inls)


class UrlEnvPlugin(EnvPlugin):
    def _doc_nodes(self) -> Sequence[type[Block] | type[Inline]]:
        return (NamedUrl,)

    @pure_fmt
    def url(self, fmt: FmtEnv, url: str, name: Optional[str] = None) -> Inline:
        if not isinstance(url, str):
            raise ValueError(f"Url {url} must be a string")
        if name is not None and not isinstance(name, str):
            raise ValueError(f"Url name {name} must be a string if not None")
        return NamedUrl(
            name=(Text(name),) if name is not None else None,
            url=url,
        )
