# turnip_text

This is a WIP language that aims to address two gripes I have with LaTeX:
1. The LaTeX macro engine is horrible to work and program with
2. LaTeX typesetting output is heavily dependent on how content is laid out in the LaTeX source file
    - i.e. a lack of separation between *content* and *formatting*

Key features include:
- Output to LaTeX, markdown, plain text formats with idiomatic code 
  - (e.g. output source files can easily be tweaked by hand)
- [Opinionated text structuring, to eliminate common LaTeX pitfalls](notes/opinionated_text.md)
- [Integrates into Python to replace LaTeX macro programming](notes/code_syntax.md)
- [Separation of content (text with embedded Python, figure content) from formatting (e.g. page breaks, figure placement, etc)](notes/content_v_formatting.md)

## License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](./LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](./LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

# Project Structure 
Based on the recommended structure from [github.com/PyO3/maturin].

- [`notes`](./notes/) - Notes on how the language should work
- [`examples`](./examples/) - Example documents, volatile and WIP
- [`python/turnip_text`](./python/turnip_text) - Python component
- [`src`](./src/) - Rust component

# FAQs

### Why output LaTeX source code?
Because LaTeX is still in many cases the lingua franca of scientific papers, it is unreasonable to expect to replace it.
The TeX typesetting engine is also still great for laying out raw text, and I have no intention of reinventing that wheel.
My problem with LaTeX is that it's finnicky, not that it produces bad output.
With all this in mind, I'd like this language to create idiomatic LaTeX source files that can then be adjusted manually and passed on to journals.

### Why output source code for other text languages, e.g. Markdown?
Recently I had to convert my masters thesis to plain text for a contest submission.
I used [Pandoc](https://pandoc.org), which is an excellent piece of software, but I ran into issues.
First, the LaTeX to Markdown conversion was extremely messy, with Pandoc-specific `{:: ::}` blocks strewn throughout the code to retain as much information as possible.
After cleaning that up I decided to work in Markdown rather than LaTeX, and use Pandoc to convert to plain text and handle footnotes and citations.
This was mostly fine, until I realized I wanted to publish the summary on my blog as well.

My blog uses Jekyll and GitHub Pages so it can't handle all of Pandoc's markdown features, particularly citations.
I was hoping to get Pandoc to take my Markdown, resolve the footnotes and citations, add a ToC, add section numbering, and save that output to Markdown again.
To my knowledge, this is impossible - I ended up having to Pandoc to raw HTML.
I would like my language to use the same front-end syntax and output to plain, everything-can-read-it Markdown, plain text, and perhaps more.

### Why not use Markdown instead?
I'm not against using Pandoc for some conversions, e.g. I'd be happy to have my language create Markdown which Pandoc turns into HTML, but Markdown isn't a suitable replacement language for LaTeX by itself.
Pandoc extensions can make it theoretically possible for Markdown to represent the same documents as LaTeX, but it's too cumbersome to work with directly imo.

### Why replace LaTeX macros with Python?
While the LaTeX macro engine is horrible to work with, it *is* still useful to embed a programming language into text.
This allows for e.g. [smart citations](http://tug.ctan.org/tex-archive/macros/latex/contrib/cleveref/cleveref.pdf) and [collecting TODO messages and notes](https://github.com/theturboturnip/latex-turnip-pkgs).
The annoyance of LaTeX is that I spend more time figuring out the macro language than I do figuring out any code.
With that in mind, I'd like the language to allow embedding a proper programming language like Python or Lua (see [notes/code_syntax.md](notes/code_syntax.md) for notes on which languages to embed).


