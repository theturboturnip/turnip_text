//! This module only performs tests when not present as an extension module.
//! test_parser and test_lexer_parser need to run Python, but aren't invoked thru python, so need to be compiled with embedded Python.
//! This means tests only work when run with `cargo test --no-default-features`.
//! All tests are disabled unless compiled without the feature to make it clear that's necessary.
#![cfg(all(test, not(feature = "extension-module")))]

mod helpers;
mod test_lexer;
mod test_lexer_parser;
mod test_parser;
