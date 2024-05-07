from typing import Iterable, List

from turnip_text import Document, parse_file
from turnip_text.build_system import BuildSystem
from turnip_text.doc.std_plugins import DocAnchors
from turnip_text.env_plugins import DocEnv, EnvPlugin, FmtEnv


class EnvSetup:
    build_sys: BuildSystem
    doc_project_relative_path: str
    plugins: List[EnvPlugin]
    fmt: FmtEnv
    doc_env: DocEnv
    anchors: DocAnchors

    def __init__(
        self,
        build_sys: BuildSystem,
        doc_project_relative_path: str,
        plugins: Iterable["EnvPlugin"],
    ) -> None:
        self.build_sys = build_sys
        self.doc_project_relative_path = doc_project_relative_path
        self.anchors = DocAnchors()
        self.plugins = list(plugins)
        self.plugins.append(self.anchors)
        self.fmt, self.doc_env = EnvPlugin._make_contexts(build_sys, self.plugins)

    def parse(self) -> Document:
        src = self.build_sys.resolve_turnip_text_source(self.doc_project_relative_path)
        return parse_file(src, self.doc_env.__dict__)

    def freeze(self) -> None:
        self.doc_env._frozen = True
