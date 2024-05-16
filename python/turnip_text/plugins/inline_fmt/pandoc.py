from typing import List

import turnip_text.render.pandoc.pandoc_types as pan
from turnip_text import Inlines, Text
from turnip_text.build_system import BuildSystem
from turnip_text.env_plugins import FmtEnv
from turnip_text.plugins.inline_fmt import (
    InlineFormatEnvPlugin,
    InlineFormatted,
    InlineFormattingType,
)
from turnip_text.render.pandoc import (
    PandocPlugin,
    PandocRenderer,
    PandocSetup,
    null_attr,
)


class PandocInlineFormatPlugin(PandocPlugin, InlineFormatEnvPlugin):
    def _register(self, build_sys: BuildSystem, setup: PandocSetup) -> None:
        super()._register(build_sys, setup)
        setup.makers.register_inline(InlineFormatted, self._build_inline_formatted)

    def _build_inline_formatted(
        self,
        inl: InlineFormatted,
        renderer: PandocRenderer,
        fmt: FmtEnv,
    ) -> pan.Inline:
        match inl.format_type:
            case InlineFormattingType.Italic | InlineFormattingType.Emph:
                return pan.Emph(renderer.make_inline_scope_list(inl.contents))
            case InlineFormattingType.Bold | InlineFormattingType.Strong:
                return pan.Strong(renderer.make_inline_scope_list(inl.contents))
            case InlineFormattingType.Underline:
                return pan.Underline(renderer.make_inline_scope_list(inl.contents))
            case InlineFormattingType.SingleQuote:
                return pan.Quoted(
                    pan.SingleQuote(), renderer.make_inline_scope_list(inl.contents)
                )
            case InlineFormattingType.DoubleQuote:
                return pan.Quoted(
                    pan.DoubleQuote(), renderer.make_inline_scope_list(inl.contents)
                )
            case InlineFormattingType.Mono:
                # Pandoc supports monospace through Code(Attr, Text). Can't have extra formatting inside.
                # Wherever possible, lift other formatting outside the Code to maintain user intent.
                # This is how Pandoc supports LaTeX constructs like \\texttt{x, \\textbf{y}} - Code("x, "), Strong(Code("y")) - it pulls the other formatting out.
                # Although interestingly if you do \\texttt{\\footnote{fred}} "fred" gets coded, not the footnote ref. lol.
                items: List[pan.Inline] = []
                for i in inl.contents:
                    if isinstance(i, Text):
                        items.append(pan.Code(null_attr(), i.text))
                    elif isinstance(i, InlineFormatted):
                        # This is InlineFormatted(Code, content=InlineFormatted(other)).
                        # Make it the other way around.
                        items.append(
                            self._build_inline_formatted(
                                InlineFormatted(
                                    format_type=i.format_type,
                                    contents=Inlines(
                                        [
                                            InlineFormatted(
                                                format_type=InlineFormattingType.Mono,
                                                contents=i.contents,
                                            )
                                        ]
                                    ),
                                ),
                                renderer,
                                fmt,
                            )
                        )
                    else:
                        # Give some content, just without any code formatting
                        # TODO could have some way to pull the text out of these and monospace it?
                        print(
                            f"Pandoc doesn't support arbitrary content inside monospace (i.e. code) - ignoring monospace"
                        )
                        items.append(renderer.make_inline(i))

                if len(items) == 1:
                    return items[0]
                return pan.Span(null_attr(), items)
            case _:
                raise ValueError(f"Unsupported formatting {inl.format_type}")

    Mono = 7
