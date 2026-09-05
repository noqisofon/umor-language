//! Umor 言語処理系のライブラリクレート。
//!
//! 字句解析器（tokenizer）と構文解析器（parser）を提供する。
//! 単語辞書・スタックマシン・型検査器は後続タスクで実装する。

pub mod parser;
pub mod tokenizer;

pub use parser::{check_scopes, parse, Definition, Expr, ParseError, Program, ScopeError};
pub use tokenizer::{tokenize, LexError, Token, TokenKind};
