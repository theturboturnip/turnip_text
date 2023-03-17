from typing import Iterator


class UnescapedText:
    @property
    def text(self) -> str: ...

class RawText:
    @property
    def contents(self) -> str: ...

class Sentence:
    def __len__(self) -> int: ...
    # Iterate over the inline blocks in the sentence
    def __iter__(self) -> Iterator: ...
    # Push an inline node into the sentence
    # TODO CHECK THEY'RE INLINE NODES
    def push_node(self, node): ...

class Paragraph:
    def __len__(self) -> int: ...
    # Iterate over the sentences in the Paragraph
    def __iter__(self) -> Iterator[Sentence]: ...
    # Push a sentence into the Paragraph
    def push_sentence(self, s: Sentence): ...


