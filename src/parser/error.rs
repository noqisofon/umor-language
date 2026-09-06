//! パーサーのエラー型。

use crate::tokenizer::Token;
use std::fmt;

/// 構文解析エラー。位置情報（トークン列インデックス・行番号）を保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    /// エラーが発生した位置のトークンインデックス。
    pub token_index: usize,
    /// エラーが発生した位置の行番号（1始まり）。入力が空の場合は0。
    pub line: usize,
}

impl ParseError {
    pub(crate) fn new(message: impl Into<String>, tokens: &[Token], token_index: usize) -> Self {
        let line = tokens
            .get(token_index)
            .or_else(|| tokens.last())
            .map(|t| t.line)
            .unwrap_or(0);
        ParseError {
            message: message.into(),
            token_index,
            line,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "構文エラー（{}行目、トークン#{}）: {}",
            self.line, self.token_index, self.message
        )
    }
}

impl std::error::Error for ParseError {}
