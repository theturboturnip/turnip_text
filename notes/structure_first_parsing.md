2023-09-23

# Structure-first parsing
New plan.
The Python parsing should be entirely stateless.
Python parsing should create the equivalent of a docbook-esque tree, do a formatting pass which may rearrange figures and add page breaks etc, a counting pass, and *then* you do a renderer-specific rendering pass.
The formatting pass is renderer independent and isn't just passive "for this version of the document arrange things like this" - it may involve structure changes. e.g. With the TODO plugin, whether you create a TODO section depends on whether there are any TODOs in the document. That decision, and the addition of the TODO structure section, must happen after parsing but before counting.

## Changes required
- We need implicit structure
  - This is my opportunity to unify the whole structure system.
  - Each implicit-structure thing has an INLINE title.
    - TODO this can be converted to ASCII and whatever format certain things want thru "
  - Need to consider how e.g. appendices interact with this.
  - Need to consider how to define what the top-level is. Already have issues trying to reuse renderers/plugins with "are we using chapters or not?", and it would be nice to be able to say "please publish this sub-section as a standalone markdown file" while rebinding sub-section -> top-level.
  - Could do something by making subfiles only able to declare sections above a certain weight? "I'm inside a thing of weight blah, so the subfile can't break out of that"...
  - Maybe if the weights were integers - not 10 apart, but 1 apart - then you just subtract from the weight if you then want to push it up. Hell, you could even look at the min/max weight in any given document before the counting phase and use that to correct any structures.
  - LESSON LEARNED FROM ATTEMPT 1: DocSegment can't be subclassed, and has to have the same gather-create pattern that blocks have instead of mutating existing DocSegments.
  - This is because for consistent protection when mutating, we need it to be subclassed, but we can't have it be subclassed because a) that screws with the type dispatch for rendering and b) it makes interacting with them through rust extremely painful
- We want metadata
  - This isn't necessary for structure-first but we should really have it.
- We need a counter system
  - Plugins need to be able to register new counted things
  - At the top level we need to be able to specify the hierarchy of counters, e.g. section counter resets when chapter is incremented, floats like figures/tables/listings/equations/whatever
- We need depth-first iterable blocks and inlines
  - This is extremely important! How tf do we do it
  - We can define the Block/Inline Rust typeclass to require iterability
  - Does iterating through a Block return the Block children or the Inlines as well?
    - for the sake of counting, it must do both
  - Fine, but how do we get user code to use this correctly?
    - It needs to be easy for a user block to define "hey, here are my sub-things"
    - Don't want to make the Rust side depend on Python-defined things.
    - Solution: UserBlock and UserInline classes that implement the Block and Inline typeclasses and take "what are my children" as an argument.
- We need to completely refactor rendering again :/
  - DocPlugins - adds Block and Inline classes and functions to put them in the tree. registers that those things should be counted. registers document modifier functions that may insert new elements into the tree based on their state in the formatting pass.
    - This is allowed to store state EXTERNAL to the document. e.g. a bib manager might store the list of citations it uses, a figure plugin might store the external resources it requires, etc.
    - INTERNAL state must be collated later.
  - DocParser - collates a set of plugins, the counter hierarchy, and how counters should be translated into labels
  - RendererPlugins - register visitor functions for the counting phase and renderer functions for the renderer phase.
    - the visitor function is used for e.g. gathering a final ordered list of figures/todo items/sections, and associating a chapter heading with the counter it needs. This gathers INTERNAL state, and the INTERNAL state necessary changes depending on which renderer you use!
    - TODO are visitor functions on entry or exit?
    - For now, entry only.
  - TODO at some point do validity regexp