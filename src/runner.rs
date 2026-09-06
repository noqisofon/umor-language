//! ソース文字列を先頭から読み、ワード定義は辞書に登録し、トップレベル式は
//! その場で即座に実行する共通コア。ファイル実行CLI・REPLの両方から使われる。

use crate::error::UmorError;
use crate::interpreter::Interpreter;
use crate::parser::parse_top_level_item;
use crate::tokenizer::tokenize;

/// `src`を先頭から読み、トップレベルの要素（ワード定義／式の列）を1つずつ
/// パースしては即座に処理する。字句解析・構文解析・実行時のいずれかで
/// エラーが起きた時点で処理を打ち切り、[`UmorError`]を返す。
pub fn run_source(interp: &mut Interpreter, src: &str) -> Result<(), UmorError> {
    let tokens = tokenize(src)?;
    let mut pos = 0usize;
    while pos < tokens.len() {
        let item = parse_top_level_item(&tokens, &mut pos)?;
        interp
            .process_top_level_item(&item)
            .map_err(|e| UmorError::Runtime(interp.report(e)))?;
    }
    Ok(())
}
