from contextlib import contextmanager
from typing import Iterator, List, Optional

from turnip_text.render import Renderer

from . import pandoc_types as pan


class PandocRenderer(Renderer):
    """An implementation of Renderer that builds a `pandoc.Document` which can then be processed into arbitrary output formats."""

    meta: pan.Meta
    blocks: List[pan.Block]
