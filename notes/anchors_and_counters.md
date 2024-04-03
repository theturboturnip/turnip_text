As of 2023-11-20, we have a basic four-phase rendering system with first-class support for anchors and counters.

The counter system currently implements manual numbering, and effectively assumes no post-render support for counting/countable things.
This would be fine for markdown, but is less than idiomatic for latex.
The renderer pass hardcodes a pass where the actual "X.Y.Z" counter numbers are gathered for every object, but this may or may not be necessary.
Worse, the counter descriptions is forced to be the same for both: Counters have a "calculate counter string for number" function, but nothing like Latex where you need to describe it as "arabic" "alph" "roman" whatever.
Effectively we need a separate counter system for Markdown.
Probably we also need a separate phase for markdown renderers, which means renderers need to be able to inject phases...
Or maybe this is just for a "backref renderer" plugin to do? in which case it gets easier.