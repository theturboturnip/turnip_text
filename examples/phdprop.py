from pathlib import Path
from turnip_text import *
from turnip_text.renderers import parse_file
from turnip_text.renderers.latex import LatexRenderer

import json
class CustomEncoder(json.JSONEncoder):
    def default(self, o):
        if isinstance(o, (BlockScope, InlineScope, Paragraph, Sentence)):
            return list(o)
        if isinstance(o, UnescapedText):
            return o.text
        if hasattr(o, "__dict__"):
            d = vars(o)
            d["str"] = str(o)
            return d
        return str(o)


if __name__ == '__main__':
    r = LatexRenderer()
    # r.load_cites("phdprop.bibtex")
    doc_block = parse_file(Path("./examples/phdprop.ttxt"), r)

    print(json.dumps(doc_block, indent=4, cls=CustomEncoder))