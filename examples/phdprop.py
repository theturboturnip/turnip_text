from pathlib import Path
from turnip_text import *
from turnip_text.renderers.latex import LatexRenderer
from turnip_text.renderers.latex.plugins import LatexCitationPlugin, LatexFootnotePlugin, LatexFormatPlugin, LatexListPlugin, LatexSectionPlugin, LatexUrlPlugin

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
    r = LatexRenderer([
        LatexCitationPlugin(),
        LatexFootnotePlugin(),
        LatexSectionPlugin(),
        LatexFormatPlugin(),
        LatexListPlugin(),
        LatexUrlPlugin()
    ])
    # r.load_cites("phdprop.bibtex")
    doc_block = r.parse_file(Path("./examples/phdprop.ttxt"))
    print(r.render_doc(doc_block))

    # print(json.dumps(doc_block, indent=4, cls=CustomEncoder))