//! 字句解析・構文解析・実行時のエラーをまとめる統一エラー型。
//!
//! [`crate::run_source`]（ファイル実行・REPL共通のソース処理コア）が返す。

use crate::interpreter::RuntimeErrorReport;
use crate::parser::ParseError;
use crate::tokenizer::LexError;
use std::fmt;

/// ソース処理中に起こりうるエラーの種別をまとめたもの。
#[derive(Debug, Clone)]
pub enum UmorError {
    Lex(LexError),
    Parse(ParseError),
    Runtime(RuntimeErrorReport),
}

impl fmt::Display for UmorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UmorError::Lex(e) => write!(f, "{e}"),
            UmorError::Parse(e) => write!(f, "{e}"),
            UmorError::Runtime(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for UmorError {}

impl From<LexError> for UmorError {
    fn from(e: LexError) -> Self {
        UmorError::Lex(e)
    }
}

impl From<ParseError> for UmorError {
    fn from(e: ParseError) -> Self {
        UmorError::Parse(e)
    }
}
