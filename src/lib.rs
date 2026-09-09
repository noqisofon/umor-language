//! Umor 言語処理系のライブラリクレート。
//!
//! 字句解析器（tokenizer）・構文解析器（parser）・評価器（interpreter）を
//! 提供する。型検査器は後続タスクで実装する。

mod error;
pub mod interpreter;
pub mod parser;
mod runner;
pub mod tokenizer;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub use error::UmorError;
pub use interpreter::{
    BufferSink, ExecutionOutcome, Interpreter, OutputSink, RuntimeError, RuntimeErrorReport,
    StdoutSink, Value,
};
pub use parser::{
    check_scopes, parse, parse_top_level_item, Definition, Expr, ParseError, Program, ScopeError,
    TopLevelItem,
};
pub use runner::run_source;
pub use tokenizer::{tokenize, LexError, Token, TokenKind};
