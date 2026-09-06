//! Umor 言語処理系のライブラリクレート。
//!
//! 字句解析器（tokenizer）・構文解析器（parser）・評価器（interpreter）を
//! 提供する。型検査器は後続タスクで実装する。

pub mod interpreter;
pub mod parser;
pub mod tokenizer;

pub use interpreter::{Interpreter, RuntimeError, RuntimeErrorReport, Value};
pub use parser::{check_scopes, parse, Definition, Expr, ParseError, Program, ScopeError};
pub use tokenizer::{tokenize, LexError, Token, TokenKind};
