from turnip_text import TurnipTextSource
from turnip_text.env_plugins import DocEnv, EnvPlugin, in_doc


class SubfileEnvPlugin(EnvPlugin):
    @in_doc
    def subfile(self, doc_env: DocEnv, project_relative_path: str) -> TurnipTextSource:
        return doc_env.build_sys.resolve_turnip_text_source(project_relative_path)
