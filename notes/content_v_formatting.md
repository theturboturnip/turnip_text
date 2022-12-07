# Content v. formatting

LaTeX attempts to be two things at once:
1. A language for expressing the content of a document
2. A typesetter/placement engine that squishes that content into a set of evenly-sized pages.

Knuth's macro design is great for 2, but not for 1 - particularly when the output of 1 must be ideal input for 2.
Examples of this are page breaks and floats.
LaTeX places these automatically, but it may not place them well.
If you have limited pages, and particularly if you have pages that are tightly packed with content, floats may need to be moved around in the source file and manual page breaks and page clears may need to be added.
This doesn't change your actual content whatsoever, so why should it change the source code for your content?

I want this language to separate the content file from specific formatting overrides.
e.g. the content file should just declare *what* figures exist, and then if the placement is bad you can use a second file to control exactly where it gets emitted in the output LaTeX code.
Still spitballing the format tho

This is also a good way to even separate the content from the *format* of the document.
It should be possible to export the same text to e.g. LaTeX or Markdown.
It's not feasible to implement support for every LaTeX package in my new language, 

Example: [the LaTeX wiki has a subsection on manual page formatting](https://en.wikibooks.org/wiki/LaTeX/Page_Layout#Manual_page_formatting), which explicitly calls out that the page break algorithm is less than ideal for long form content and requires manual adjustment.

You can adjust the weight for LaTeX to avoid widow and orphan lines (single lines at the top/bottom of the page), which is something I usually use manual page breaks or content changes to address.
I don't think this is a complete substitute.
The most damning problem is that usually this is due to the content literally taking up too much space.
If the content is going to fit in the space, the content *must* be changed to fit in the space.
This could be another interesting opportunity for the "adjustment" file - minor sentence-based content changes?

## Goals of this feature
- Insert figures and page breaks in specific places within content, without requiring the content itself to be changed
  - This hopefully wouldn't require manual insertion of labels in the content text
  - Thus would need a way to reference specific paragraphs and lines 
  - e.g. "insert figure blah (that must be manually labelled) in the output text after the end of paragraph X"
  - or "insert page break in the output text directly after paragraph X line Y"
- Insert format-specific code in preamble, postamble, between content
  - In some cases this could be handled by the embedded language in the content (e.g. citations will need to be handled at some level in the content)
  - but things like setting a specific document class/size using preamble code could be cool
  - and this could be a useful general case of inserting page breaks
  - In that case, using paragraph/line anchors would define a good splitting point between what should be handled in embedded/manually inserted code and what should be handled in the overlay file. Anything inline = embedded or manual code, anything between lines or paragraphs in the overlay
  - Also encourages proper sentence/line splitting (see Opinionated Text)
- Replace sentences in the content for one specific version
  - Not difficult to do if paragraph/line anchoring is the norm
  - Problem - this complicates the distinction between what is content and what isn't
  - only suitable on the sentence level, if that
  - anything more should be a separate content file
  - i.e. "magazine ver" content file is still a separate file from "book ver"
  - but i think the split is still valuable there? especially for multiformat markdown vs latex

Implicit anchors
- by paragraph
- by paragraph+line
- by paragraph(+line)+content?
  - as in the sentence in paragraph blah that starts with "XYZ"?

## Potential example
### `source.ltxt`
```
[section("chap:bg:sec:rvvmemory")]{RVV memory instructions}
[emph]{Summarizes~[cite("specification-RVV-v1.0", "Sections~7-9")]}

RVV defines three broad categories of memory access instructions, which can be further split into five archetypes with different semantics.
This section summarizes each archetype, their semantics, their assembly mnemonics, and demonstrates how they map memory accesses to vector elements.


For the most part, memory access instructions handle their operands as described in [cref("chap:bg:sec:rvv:vector_model")].
[code]{EEW} and [code]{EMUL} are usually derived from the instruction encoding, rather than reading the [code]{vtype} CSR.
In a few cases the Effective Vector Length [code]{EVL} is different from the [code]{vl} CSR, so for simplicity all instructions are described in terms of [code]{EVL}.

[subsection(num=False)]{Segmented accesses}
Three of the five archetypes (unit/strided, fault-only-first, and indexed) support [emph]{segmented} access.
# TODO how to insert other text stuff e.g. [code] inside LaTeX math? is this OK?
This is used for unpacking contiguous structures of [math]{1 \le [code]{nf} \le 8} [emph]{fields} and placing each field in a separate vector.
In these instructions, the values of [code]{vl}, [code]{vstart}, and the mask register are interpreted in terms of segments.

# Note that the caption here uses \n, ideally we could turn that into \\ for the caption
# TODO r{ raw syntax isn't great
[code]r{
    class RVVMemUnitFigure(Figure):
        def __init__(self, subfig_width: Dimension, pos: FigPos):
            super().__init__(pos)
            self.subfig_width = subfig_width
        
        # TODO some function that creates a layout that can be LaTeX converted, I guess
}
# NOTE put the caption in text scope so you can put formatting in it
[add_figure(
    "fig:RVV_mem_unit",
    RVVMemUnitFigure,
    # Default kwargs for the figure
    subfig_width=text_width * 0.48,
    pos=FigPos.BOTTOM
)]{
Comparison between segmented and unsegmented accesses
For readability, the vector registers are 2x as wide
}

# A normal figure
#[add_figure("fig:default", ImageFigure, img="RVV_mem_unit_noseg.pdf")]{Simple vector element to address mapping}

[cref("fig:RVV_mem_unit")] demonstrates a common example: the extraction of separate R, G, and B components from a color.
Without segmentation, i.e. [math]{n = 1}, each consecutive memory address maps to a consecutive element in a single vector register group.
With segmentation, elements are grouped into segments of [math]{n > 1} fields, where each field is mapped to a different vector register group.
This principle extends to [code]{LMUL > 1} ([cref("fig:RVV_mem_lmul_3seg")]).
```
### `book.ltyp`
```
# Get content from the given file
[addcontent("source.ltxt")]

# Insert LaTeX code directly to induce a pagebreak
[emit("latex", before=lookup("chap:bg:sec:rvvmemory", para=1))]r{\pagebreak}

# Place the figure (if it wasn't placed, it would appear where it was originally defined)
[emitfig(
    "fig:RVV_mem_unit",
    after=lookup(subsection="Segmented accesses", para=2),
    # other kwargs get passed though to figure constructor, override defaults
    pos=FigPos.HERE
)]
```
### Original LaTeX
`sub35_rvv_memory.tex`
```latex
\section{RVV memory instructions\label{chap:bg:sec:rvvmemory}}
\emph{Summarizes~\cite[Sections~7-9]{specification-RVV-v1.0}}

RVV defines three broad categories of memory access instructions, which can be further split into five archetypes with different semantics.
This section summarizes each archetype, their semantics, their assembly mnemonics, and demonstrates how they map memory accesses to vector elements.


For the most part, memory access instructions handle their operands as described in \cref{chap:bg:sec:rvv:vector_model}.
\code{EEW} and \code{EMUL} are usually derived from the instruction encoding, rather than reading the \code{vtype} CSR.
In a few cases the Effective Vector Length \code{EVL} is different from the \code{vl} CSR, so for simplicity all instructions are described in terms of \code{EVL}.

\subsection*{Segmented accesses}
Three of the five archetypes (unit/strided, fault-only-first, and indexed) support \emph{segmented} access.
This is used for unpacking contiguous structures of $1 \le \code{nf} \le 8$ \emph{fields} and placing each field in a separate vector.
In these instructions, the values of \code{vl}, \code{vstart}, and the mask register are interpreted in terms of segments.

\cref{fig:RVV_mem_unit} demonstrates a common example: the extraction of separate R, G, and B components from a color.
Without segmentation, i.e. $n = 1$, each consecutive memory address maps to a consecutive element in a single vector register group.
With segmentation, elements are grouped into segments of $n > 1$ fields, where each field is mapped to a different vector register group.
This principle extends to \code{LMUL > 1} (\cref{fig:RVV_mem_lmul_3seg}).

\figinput[width=0.48\textwidth,pos=h]{1_20Background/figures/fig_RVV_mem_unit}
```

`fig_RVV_mem_unit.tex`
```latex
\begin{turnipfig}
    \centering
    \begin{subfigure}[t]{\figinputWidth}
        \centering
        \adjustbox{valign=t}{\includegraphics[width=\textwidth]{Figures/RVV_mem_unit_noseg.pdf}}
        \caption{Simple vector element to address mapping}
        \label{fig:RVV_mem_unit_noseg}
    \end{subfigure}
    \hfill
    \begin{subfigure}[t]{\figinputWidth}
        \centering
        \adjustbox{valign=t}{\includegraphics[width=\textwidth]{Figures/RVV_mem_unit_3seg.pdf}}
        \caption{Element-address mapping for segmented access}
        \label{fig:RVV_mem_unit_3seg}
    \end{subfigure}
    
    \begin{subfigure}[t]{\figinputWidth}
        \centering
     \includegraphics[width=\textwidth]{Figures/RVV_mem_lmul_3seg.pdf}
        \caption{Example of segment mapping for \code{LMUL > 1}}
        \label{fig:RVV_mem_lmul_3seg}
    \end{subfigure}
    \caption{Comparison between segmented and unsegmented accesses\\For readability, the vector registers are 2x as wide}
    \label{fig:RVV_mem_unit}
\end{turnipfig}
```
