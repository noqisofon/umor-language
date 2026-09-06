//! 字句解析エラーの型。

use std::fmt;

/// 字句解析エラー。位置情報（行・列）を保持する。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    /// エラーが発生した位置の行番号（1始まり）。
    pub line: usize,
    /// エラーが発生した位置の列番号（1始まり）。
    pub column: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "字句解析エラー（{}行目{}列目）: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for LexError {}
