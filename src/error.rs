//! 字句解析・構文解析・実行時のエラーをまとめる統一エラー型。
//!
//! [`crate::run_source`]（ファイル実行・REPL共通のソース処理コア）が返す。

use crate::interpreter::RuntimeErrorReport;
use crate::parser::{ParseError, ScopeError};
use crate::tokenizer::LexError;
use std::fmt;

/// ソース処理中に起こりうるエラーの種別をまとめたもの。
#[derive(Debug, Clone)]
pub enum UmorError {
    Lex(LexError),
    Parse(ParseError),
    Runtime(RuntimeErrorReport),
    /// ADR-0013: `run_file_source`が評価に入る前に`check_scopes`で検出した
    /// 静的スコープ違反。1件以上まとめて返す。
    Scope(Vec<ScopeError>),
}

impl fmt::Display for UmorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UmorError::Lex(e) => write!(f, "{e}"),
            UmorError::Parse(e) => write!(f, "{e}"),
            UmorError::Runtime(e) => write!(f, "{e}"),
            UmorError::Scope(errors) => {
                let messages: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
                write!(f, "{}", messages.join("\n"))
            }
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
