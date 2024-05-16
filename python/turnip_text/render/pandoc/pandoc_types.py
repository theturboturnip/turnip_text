"""
The `pandoc` Python package autogenerates a bunch of types from Haskell.
For ease of use with mypy, I have written a script that generates type hints for the given version of the package.
I import the types through this package to get those type hints.
Currently it's for version 2.4 I believe.
"""

from pandoc.types import *  # type: ignore
