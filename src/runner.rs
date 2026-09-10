//! ソース文字列を先頭から読み、ワード定義は辞書に登録し、トップレベル式は
//! その場で即座に実行する共通コア。ファイル実行CLI・REPLの両方から使われる。

use crate::error::UmorError;
use crate::interpreter::{ExecutionOutcome, Interpreter};
use crate::parser::{check_scopes, parse_top_level_item, Definition, Program, TopLevelItem};
use crate::tokenizer::tokenize;

/// `src`を先頭から読み、トップレベルの要素（ワード定義／式の列）を1つずつ
/// パースしては即座に処理する（逐次パース・逐次実行）。字句解析・構文解析・
/// 実行時のいずれかでエラーが起きた時点で処理を打ち切り、[`UmorError`]を
/// 返す。REPL・`含める`／`必要`（モジュール読み込み）から共有して使われる
/// ため、静的スコープチェック（ADR-0013）は組み込まない
/// （ファイル実行専用の[`run_file_source`]を参照）。
///
/// 途中で`終了`・`さよなら`（ADR-0001）が実行され[`ExecutionOutcome::Exit`]が
/// 返された場合は、それ以降のトップレベル要素を評価せずに`Ok(ExecutionOutcome::Exit)`を
/// 返す。エラーではないので、呼び出し元（REPLループ・ファイル実行）はこれを
/// 正常終了として扱ってよい。
pub fn run_source(interp: &mut Interpreter, src: &str) -> Result<ExecutionOutcome, UmorError> {
    let tokens = tokenize(src)?;
    let mut pos = 0usize;
    while pos < tokens.len() {
        let item = parse_top_level_item(&tokens, &mut pos)?;
        match interp
            .process_top_level_item(&item)
            .map_err(|e| UmorError::Runtime(interp.report(e)))?
        {
            ExecutionOutcome::Continue => {}
            ExecutionOutcome::Exit => return Ok(ExecutionOutcome::Exit),
        }
    }
    Ok(ExecutionOutcome::Continue)
}

/// ファイル実行専用の入口（ADR-0013の暫定方針・案A）。`src`全体を先に
/// トークナイズ・パースし切ってから、その中のワード定義全体へ
/// [`check_scopes`]を通す。違反が1件でもあれば、一切評価に入らずに
/// [`UmorError::Scope`]を返す。違反がなければ、パース済みのトップレベル
/// 要素を先頭から順に評価する（このロジック自体は[`run_source`]と同じ）。
///
/// `run_source`とは異なり全体パース→静的チェック→評価というフェーズ分け
/// を行うため、1行ずつ供給される入力（REPL・`含める`／`必要`）には使えない。
/// それらは引き続き`run_source`を使う。
pub fn run_file_source(interp: &mut Interpreter, src: &str) -> Result<ExecutionOutcome, UmorError> {
    let tokens = tokenize(src)?;
    let mut pos = 0usize;
    let mut items = Vec::new();
    while pos < tokens.len() {
        items.push(parse_top_level_item(&tokens, &mut pos)?);
    }

    let definitions: Vec<Definition> = items
        .iter()
        .filter_map(|item| match item {
            TopLevelItem::Definition(def) => Some(def.clone()),
            TopLevelItem::Expr(_) => None,
        })
        .collect();
    check_scopes(&Program { definitions }).map_err(UmorError::Scope)?;

    for item in &items {
        match interp
            .process_top_level_item(item)
            .map_err(|e| UmorError::Runtime(interp.report(e)))?
        {
            ExecutionOutcome::Continue => {}
            ExecutionOutcome::Exit => return Ok(ExecutionOutcome::Exit),
        }
    }
    Ok(ExecutionOutcome::Continue)
}
