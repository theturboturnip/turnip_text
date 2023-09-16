# The idiomatic way to label

## LaTeX

```latex
\section{Section}\label{blah}

Section~\ref{blah} % Section~1

Page~\pageref{blah} % Page~X

% (context dependent)
\cref{blah} % Section~1
```

Calling `\label{name}` associates `name` with the value of the last-incremented LaTeX counter, and the location in the document.
`\ref{blah}` returns the value associated with `name`, optionally with a hyperlink to the associated location in the PDF.
`\pageref{blah}` returns the page number for the associated location.
`\cref{blah}`, based on `\usepackage{cleveref}`, hooks `\ref` to also record the kind of the last-incremented counter and exploits that to return a string fully identifying the label (and a hyperlink).
`cleveref` also supports passing multiple references in, and automatically sorting them into a nice-looking setup.


TODO autoref, varioref? They seem like subsets of cleveref.

## Typst

TODO