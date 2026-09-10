//! Umorの構文解析器（パーサー）。
//!
//! 字句解析済みのトークン列を受け取り、[`Program`]（AST）を構築する。
//! 対応する構文要素はフェーズ1の範囲（実装指示書参照）:
//!
//! 1. ワード定義（`とは`/`は` 〜 `。`）
//! 2. 局所処理単語（`。`を付けない内部定義、`本体とは`必須ルール）
//! 3. 通常のワード呼び出し列
//! 4. 条件分岐（`ならば`/`そうでなければ`/`つぎに`）
//! 5. 変数宣言（`Xは 変数`）
//! 6. `（）`の添字アクセス糖衣構文の脱糖衣
//!
//! 変数の兄弟スコープチェックはパースとは別パス（[`scope`]モジュール）に
//! 分離している。まずは構文的に正しいASTを組み立てることを優先する。

mod ast;
mod error;
pub mod scope;

pub use ast::{Definition, Expr, Program, TopLevelItem};
pub use error::ParseError;
pub use scope::{check_scopes, ScopeError};

use crate::tokenizer::{Token, TokenKind};

/// トークン列を構文解析し、[`Program`]を返す。
///
/// トップレベルにワード定義しか書けない前提のエントリポイント。トップレベル
/// 即時実行式も扱いたい場合は[`parse_top_level_item`]をループで呼び出すこと
/// （[`crate::run_source`]が、それをファイル実行・REPL向けに行っている）。
pub fn parse(tokens: &[Token]) -> Result<Program, ParseError> {
    let mut pos = 0usize;
    let mut definitions = Vec::new();
    while pos < tokens.len() {
        definitions.push(parse_definition(tokens, &mut pos, false)?);
    }
    Ok(Program { definitions })
}

/// トークン列の現在位置から、トップレベルの要素（ワード定義、または
/// その場で実行される式の列）を1つだけ解析し、位置を進めて返す。
///
/// 先頭が`〈単語〉 とは`/`〈単語〉 は`の形であればワード定義として、
/// そうでなければ`。`までの式の列として解析する。
pub fn parse_top_level_item(tokens: &[Token], pos: &mut usize) -> Result<TopLevelItem, ParseError> {
    if let Some(varname) = try_variable_decl(tokens, pos) {
        // 「Xは 変数」の直後は「。」で閉じる想定（トップレベルの1要素として完結する）。
        expect_word(tokens, pos, "。")?;
        return Ok(TopLevelItem::Expr(vec![Expr::VariableDecl(varname)]));
    }
    // 「Xは 可変値で 〈式〉が 初期値」の判定は、`peek_word_then_keyword`
    // （ワード定義開始判定）より必ず先に試す必要がある（「Xは」という
    // 並びがワード定義開始と誤認されるため。`変数`宣言と同じ理由）。
    if let Some(expr) = try_value_decl(tokens, pos, false)? {
        // 「初期値」の直後は「。」で閉じる想定（トップレベルの1要素として完結する）。
        expect_word(tokens, pos, "。")?;
        return Ok(TopLevelItem::Expr(vec![expr]));
    }
    if let Some((new_name, existing_name)) = try_alias_decl(tokens, pos) {
        // 「Xも Yの 別名」の直後も同様に「。」で閉じる想定。ADR-0030。
        expect_word(tokens, pos, "。")?;
        return Ok(TopLevelItem::Expr(vec![Expr::AliasDecl {
            new_name,
            existing_name,
        }]));
    }
    if peek_word_then_keyword(tokens, *pos).is_some() {
        let def = parse_definition(tokens, pos, false)?;
        return Ok(TopLevelItem::Definition(def));
    }
    let exprs = parse_top_level_expr_sequence(tokens, pos)?;
    Ok(TopLevelItem::Expr(exprs))
}

/// トップレベル式の列を、`。`が現れるまで解析する（`。`自体は消費する）。
///
/// ワード定義の`body`とは異なり、`こと。`で閉じることはできない
/// （閉じるべきワード定義がこの文脈には存在しないため、`こと。`が
/// 現れた場合は構文エラーにする）。
fn parse_top_level_expr_sequence(
    tokens: &[Token],
    pos: &mut usize,
) -> Result<Vec<Expr>, ParseError> {
    let mut exprs = Vec::new();
    loop {
        if is_word(tokens, *pos, "。") {
            *pos += 1;
            break;
        }
        if *pos >= tokens.len() {
            return Err(ParseError::new(
                "トップレベルの式の列が「。」で閉じられないまま入力が終了しました",
                tokens,
                *pos,
            ));
        }
        if is_word(tokens, *pos, "こと") && is_word(tokens, *pos + 1, "。") {
            return Err(ParseError::new(
                "「こと。」に対応するワード定義がありません",
                tokens,
                *pos,
            ));
        }

        if let Some(expr) = try_value_decl(tokens, pos, false)? {
            exprs.push(expr);
            continue;
        }
        if let Some(expr) = try_assign(tokens, pos, &mut exprs) {
            exprs.push(expr);
            continue;
        }

        if is_word(tokens, *pos, "ここから") {
            *pos += 1;
            let body = parse_loop_body(tokens, pos, false)?;
            exprs.push(Expr::InfiniteLoop { body });
            continue;
        }

        if is_word(tokens, *pos, "回数指定") {
            *pos += 1;
            let body = parse_loop_body(tokens, pos, false)?;
            exprs.push(Expr::CountedLoop { body });
            continue;
        }

        if is_word(tokens, *pos, "ならば") {
            *pos += 1;
            let cond = std::mem::take(&mut exprs);
            let then_branch = parse_branch(tokens, pos, false)?;
            let else_branch = if is_word(tokens, *pos, "そうでなければ") {
                *pos += 1;
                Some(parse_branch(tokens, pos, false)?)
            } else {
                None
            };
            expect_word(tokens, pos, "つぎに")?;
            exprs.push(Expr::IfElse {
                cond,
                then_branch,
                else_branch,
            });
            continue;
        }

        parse_atom_with_subscripts(tokens, pos, &mut exprs, false)?;
    }
    Ok(exprs)
}

fn word_at(tokens: &[Token], pos: usize) -> Option<&str> {
    match tokens.get(pos).map(|t| &t.kind) {
        Some(TokenKind::Word(w)) => Some(w.as_str()),
        _ => None,
    }
}

fn is_word(tokens: &[Token], pos: usize, s: &str) -> bool {
    word_at(tokens, pos) == Some(s)
}

fn is_defining_keyword(tokens: &[Token], pos: usize) -> bool {
    matches!(word_at(tokens, pos), Some("とは") | Some("は"))
}

/// `tokens[pos]`が単語で、かつ`tokens[pos + 1]`が`とは`/`は`である場合、
/// その単語を返す（変数宣言 `Xは 変数` との区別は呼び出し側の責務）。
fn peek_word_then_keyword(tokens: &[Token], pos: usize) -> Option<&str> {
    let w = word_at(tokens, pos)?;
    if is_defining_keyword(tokens, pos + 1) {
        Some(w)
    } else {
        None
    }
}

fn is_open_paren(tokens: &[Token], pos: usize) -> bool {
    matches!(tokens.get(pos).map(|t| &t.kind), Some(TokenKind::OpenParen))
}

fn is_close_paren(tokens: &[Token], pos: usize) -> bool {
    matches!(
        tokens.get(pos).map(|t| &t.kind),
        Some(TokenKind::CloseParen)
    )
}

fn expect_any_word(tokens: &[Token], pos: &mut usize) -> Result<String, ParseError> {
    match tokens.get(*pos).map(|t| &t.kind) {
        Some(TokenKind::Word(w)) => {
            let w = w.clone();
            *pos += 1;
            Ok(w)
        }
        _ => Err(ParseError::new("単語が必要です", tokens, *pos)),
    }
}

fn expect_defining_keyword(tokens: &[Token], pos: &mut usize) -> Result<(), ParseError> {
    if is_defining_keyword(tokens, *pos) {
        *pos += 1;
        Ok(())
    } else {
        Err(ParseError::new(
            "「とは」または「は」が必要です",
            tokens,
            *pos,
        ))
    }
}

fn expect_word(tokens: &[Token], pos: &mut usize, s: &str) -> Result<(), ParseError> {
    if is_word(tokens, *pos, s) {
        *pos += 1;
        Ok(())
    } else {
        Err(ParseError::new(format!("「{s}」が必要です"), tokens, *pos))
    }
}

/// `Xは 変数` を認識し、`X`を返す。マッチしなければ何も消費せず`None`。
fn try_variable_decl(tokens: &[Token], pos: &mut usize) -> Option<String> {
    let name = word_at(tokens, *pos)?;
    if is_word(tokens, *pos + 1, "は") && is_word(tokens, *pos + 2, "変数") {
        let name = name.to_string();
        *pos += 3;
        Some(name)
    } else {
        None
    }
}

/// `Xは 可変値` / `Xは 定数値` のヘッダ部分を認識し、`(X, is_constant)`を
/// 返す。マッチしなければ何も消費せず`None`。
///
/// 注意（トークナイザーの送り仮名正規化）: `可変値で`は活用語尾の`で`が
/// 送り仮名として吸収され、単一トークン`可変値`に正規化される（`定数値で`
/// も同様）。そのため語幹`可変値`／`定数値`単独への一致で判定してよい。
fn try_value_decl_header(tokens: &[Token], pos: usize) -> Option<(String, bool)> {
    let name = word_at(tokens, pos)?;
    if !is_word(tokens, pos + 1, "は") {
        return None;
    }
    if is_word(tokens, pos + 2, "可変値") {
        Some((name.to_string(), false))
    } else if is_word(tokens, pos + 2, "定数値") {
        Some((name.to_string(), true))
    } else {
        None
    }
}

/// `Xは 可変値で 〈式〉が 初期値` / `Xは 定数値で 〈式〉が 初期値` を認識し、
/// `Expr::ValueDecl`を返す。ヘッダにマッチしなければ何も消費せず`Ok(None)`。
/// ヘッダにマッチした後、初期値の式（`初期値`で終端）が正しく閉じられ
/// なければ`Err`。ADR-0020。
fn try_value_decl(
    tokens: &[Token],
    pos: &mut usize,
    in_definition: bool,
) -> Result<Option<Expr>, ParseError> {
    let Some((name, is_constant)) = try_value_decl_header(tokens, *pos) else {
        return Ok(None);
    };
    *pos += 3; // 名前 + は + 可変値/定数値
    let init_expr = parse_value_init_expr(tokens, pos, in_definition)?;
    Ok(Some(Expr::ValueDecl {
        name,
        is_constant,
        init_expr,
    }))
}

/// `可変値で`/`定数値で`の直後から`初期値`が現れるまでの式を解析する。
/// `parse_branch`と同様、ネストした`ならば`・ループも扱えるが、終端
/// キーワードが`初期値`である点が異なる。ADR-0020。
fn parse_value_init_expr(
    tokens: &[Token],
    pos: &mut usize,
    in_definition: bool,
) -> Result<Vec<Expr>, ParseError> {
    let mut exprs = Vec::new();
    loop {
        if is_word(tokens, *pos, "初期値") {
            *pos += 1;
            break;
        }
        if *pos >= tokens.len() {
            return Err(ParseError::new(
                "可変値/定数値の宣言が「初期値」で閉じられないまま入力が終了しました",
                tokens,
                *pos,
            ));
        }

        if let Some(expr) = try_value_decl(tokens, pos, in_definition)? {
            exprs.push(expr);
            continue;
        }
        if let Some(expr) = try_assign(tokens, pos, &mut exprs) {
            exprs.push(expr);
            continue;
        }

        if is_word(tokens, *pos, "ここから") {
            *pos += 1;
            let body = parse_loop_body(tokens, pos, in_definition)?;
            exprs.push(Expr::InfiniteLoop { body });
            continue;
        }

        if is_word(tokens, *pos, "回数指定") {
            *pos += 1;
            let body = parse_loop_body(tokens, pos, in_definition)?;
            exprs.push(Expr::CountedLoop { body });
            continue;
        }

        if is_word(tokens, *pos, "ならば") {
            *pos += 1;
            let cond = std::mem::take(&mut exprs);
            let then_branch = parse_branch(tokens, pos, in_definition)?;
            let else_branch = if is_word(tokens, *pos, "そうでなければ") {
                *pos += 1;
                Some(parse_branch(tokens, pos, in_definition)?)
            } else {
                None
            };
            expect_word(tokens, pos, "つぎに")?;
            exprs.push(Expr::IfElse {
                cond,
                then_branch,
                else_branch,
            });
            continue;
        }

        parse_atom_with_subscripts(tokens, pos, &mut exprs, in_definition)?;
    }
    Ok(exprs)
}

/// `〈値の式〉 〈対象名〉 代入` を認識する。`tokens[*pos]`が`代入`であり、
/// かつ直前に積まれた`exprs`の末尾が`Expr::WordCall(name)`（＝まだ辞書引き
/// されていない対象名）であれば、それを取り除いて`Expr::Assign`を返す。
///
/// 対象名をここで横取りする理由（ADR-0031）: `可変値`の名前を通常の
/// `WordCall`として評価すると、対象を指し示す前に現在値がスタックへ
/// 積まれてしまい、代入先を区別できなくなる。パーサーが`代入`の直前
/// トークンを対象名として直接ASTノードへ埋め込むことで、ワード呼び出し
/// としての評価が起きる前に横取りする。
///
/// マッチしなければ何も消費せず`None`。
fn try_assign(tokens: &[Token], pos: &mut usize, exprs: &mut Vec<Expr>) -> Option<Expr> {
    if !is_word(tokens, *pos, "代入") {
        return None;
    }
    if !matches!(exprs.last(), Some(Expr::WordCall(_))) {
        return None;
    }
    let name = match exprs.pop() {
        Some(Expr::WordCall(name)) => name,
        _ => unreachable!("直前でExpr::WordCall(_)であることを確認済み"),
    };
    *pos += 1;
    Some(Expr::Assign {
        name,
        value_expr: Vec::new(),
    })
}

/// `〈新語〉も 〈既存語〉の 別名` を認識し、`(新語, 既存語)`を返す。
/// マッチしなければ何も消費せず`None`。ADR-0030。
fn try_alias_decl(tokens: &[Token], pos: &mut usize) -> Option<(String, String)> {
    let new_name = word_at(tokens, *pos)?;
    if is_word(tokens, *pos + 1, "も") {
        let existing_name = word_at(tokens, *pos + 2)?;
        if is_word(tokens, *pos + 3, "の") && is_word(tokens, *pos + 4, "別名") {
            let new_name = new_name.to_string();
            let existing_name = existing_name.to_string();
            *pos += 5;
            return Some((new_name, existing_name));
        }
    }
    None
}

/// 半角/全角数字・マイナス符号のみからなる数値トークンを`i64`へ変換する。
/// 小数点・指数表記はフェーズ1の`NumberLiteral(i64)`では未対応のためエラーとする。
fn parse_number_literal(raw: &str, tokens: &[Token], pos: usize) -> Result<i64, ParseError> {
    let mut ascii = String::with_capacity(raw.len());
    for c in raw.chars() {
        let mapped = match c {
            '0'..='9' | '-' => c,
            '\u{FF10}'..='\u{FF19}' => {
                char::from_u32(c as u32 - 0xFF10 + '0' as u32).expect("全角数字の変換に失敗")
            }
            '\u{FF0D}' => '-',
            _ => {
                return Err(ParseError::new(
                    format!("整数以外の数値リテラルはフェーズ1では未対応です: 「{raw}」"),
                    tokens,
                    pos,
                ));
            }
        };
        ascii.push(mapped);
    }
    ascii.parse::<i64>().map_err(|_| {
        ParseError::new(
            format!("数値リテラルの解析に失敗しました: 「{raw}」"),
            tokens,
            pos,
        )
    })
}

/// 1つのワード定義（`is_local == false`）または局所処理単語（`is_local == true`）を解析する。
///
/// 前提: `tokens[*pos]`が名前の単語、`tokens[*pos + 1]`が`とは`/`は`であること。
/// - `is_local == false`: `こと。`または`。`まで読み進めてクローズする。
/// - `is_local == true`: 次の`単語 とは/は`が先読みできた時点で（消費せず）暗黙にクローズする。
fn parse_definition(
    tokens: &[Token],
    pos: &mut usize,
    is_local: bool,
) -> Result<Definition, ParseError> {
    let name_pos = *pos;
    let name = expect_any_word(tokens, pos)?;
    if name == "再帰" {
        return Err(ParseError::new(
            "「再帰」は予約された構文キーワードのため、処理単語名として定義できません",
            tokens,
            name_pos,
        ));
    }
    expect_defining_keyword(tokens, pos)?;

    let mut locals = Vec::new();
    let mut variables = Vec::new();
    let mut body: Vec<Expr> = Vec::new();
    let mut body_started = false;

    loop {
        // クローズ判定（トップレベル/通常のワード定義）: 「こと。」または「。」。
        if !is_local && body_started {
            if is_word(tokens, *pos, "こと") && is_word(tokens, *pos + 1, "。") {
                *pos += 2;
                break;
            }
            if is_word(tokens, *pos, "。") {
                *pos += 1;
                break;
            }
        }

        if *pos >= tokens.len() {
            let msg = if is_local {
                "局所処理単語が閉じられないまま入力が終了しました（本体とは、または次の定義が必要です）"
            } else {
                "ワード定義が「。」で閉じられないまま入力が終了しました"
            };
            return Err(ParseError::new(msg, tokens, *pos));
        }

        // 変数宣言: Xは 変数（局所処理単語の中でも、本体の中でも、どこでも認識する）。
        // 「単語 とは/は」の先読み判定より必ず先に試す（「Xは」を局所処理単語の
        // 開始や暗黙クローズのトリガーと誤認しないようにするため）。
        if let Some(varname) = try_variable_decl(tokens, pos) {
            variables.push(varname);
            continue;
        }

        // 可変値/定数値の宣言・代入も、変数宣言と同様にどの文脈でも認識する。
        // 「単語 とは/は」の先読み判定（クローズ判定・局所処理単語の開始判定）
        // より必ず先に試す（「Xは 可変値で」の「Xは」を誤認しないようにするため）。
        if let Some(expr) = try_value_decl(tokens, pos, true)? {
            body.push(expr);
            continue;
        }
        if let Some(expr) = try_assign(tokens, pos, &mut body) {
            body.push(expr);
            continue;
        }

        // クローズ判定（局所処理単語）: 次が「単語 とは/は」なら消費せず暗黙にクローズする。
        //
        // 局所処理単語は、それ自身の入れ子の局所処理単語を持たない（フェーズ1の単純化）。
        // これは「直前の未クローズな定義を無条件にpopしてから新しい定義をpushする」という
        // 仕様の「pop」に相当し、制御はここで親（呼び出し元のループ）へ戻る。
        if is_local {
            if peek_word_then_keyword(tokens, *pos).is_some() {
                break;
            }
            if is_word(tokens, *pos, "。") {
                return Err(ParseError::new(
                    "局所処理単語は「。」で閉じません（本体とは、または次の局所処理単語の開始で暗黙に閉じます）",
                    tokens,
                    *pos,
                ));
            }
        }

        // 局所処理単語の開始、または「本体とは」マーカー（トップレベル/通常のワード定義のみ）。
        if !is_local {
            if let Some(word) = peek_word_then_keyword(tokens, *pos) {
                if !body_started {
                    if word == "本体" {
                        *pos += 2; // 本体 + とは/は を読み飛ばす（ASTには残らない）
                        body_started = true;
                        continue;
                    }
                    let local = parse_definition(tokens, pos, true)?;
                    locals.push(local);
                    continue;
                }
                return Err(ParseError::new(
                    format!(
                        "本体の中で予期しない局所処理単語の開始のようなもの「{word}」に達しました（局所処理単語は本体より前に置いてください）"
                    ),
                    tokens,
                    *pos,
                ));
            }
        }

        if !body_started {
            if !locals.is_empty() {
                return Err(ParseError::new(
                    "局所処理単語がある場合は「本体とは」が必要です",
                    tokens,
                    *pos,
                ));
            }
            body_started = true;
        }

        if is_word(tokens, *pos, "ここから") {
            *pos += 1;
            let loop_body = parse_loop_body(tokens, pos, true)?;
            body.push(Expr::InfiniteLoop { body: loop_body });
            continue;
        }

        if is_word(tokens, *pos, "回数指定") {
            *pos += 1;
            let loop_body = parse_loop_body(tokens, pos, true)?;
            body.push(Expr::CountedLoop { body: loop_body });
            continue;
        }

        if is_word(tokens, *pos, "ならば") {
            *pos += 1;
            let cond = std::mem::take(&mut body);
            let then_branch = parse_branch(tokens, pos, true)?;
            let else_branch = if is_word(tokens, *pos, "そうでなければ") {
                *pos += 1;
                Some(parse_branch(tokens, pos, true)?)
            } else {
                None
            };
            expect_word(tokens, pos, "つぎに")?;
            body.push(Expr::IfElse {
                cond,
                then_branch,
                else_branch,
            });
            continue;
        }

        parse_atom_with_subscripts(tokens, pos, &mut body, true)?;
    }

    Ok(Definition {
        name,
        locals,
        variables,
        body,
    })
}

/// `ならば`/`そうでなければ`の節（then節・else節）を、`そうでなければ`または`つぎに`の
/// 手前まで解析する。節の中にネストした`ならば`〜`つぎに`も再帰的に扱う。
fn parse_branch(
    tokens: &[Token],
    pos: &mut usize,
    in_definition: bool,
) -> Result<Vec<Expr>, ParseError> {
    let mut exprs = Vec::new();
    loop {
        if is_word(tokens, *pos, "そうでなければ") || is_word(tokens, *pos, "つぎに") {
            break;
        }
        if *pos >= tokens.len() {
            return Err(ParseError::new(
                "条件分岐が「つぎに」で閉じられないまま入力が終了しました",
                tokens,
                *pos,
            ));
        }

        if let Some(expr) = try_value_decl(tokens, pos, in_definition)? {
            exprs.push(expr);
            continue;
        }
        if let Some(expr) = try_assign(tokens, pos, &mut exprs) {
            exprs.push(expr);
            continue;
        }

        if is_word(tokens, *pos, "ここから") {
            *pos += 1;
            let body = parse_loop_body(tokens, pos, in_definition)?;
            exprs.push(Expr::InfiniteLoop { body });
            continue;
        }

        if is_word(tokens, *pos, "回数指定") {
            *pos += 1;
            let body = parse_loop_body(tokens, pos, in_definition)?;
            exprs.push(Expr::CountedLoop { body });
            continue;
        }

        if is_word(tokens, *pos, "ならば") {
            *pos += 1;
            let cond = std::mem::take(&mut exprs);
            let then_branch = parse_branch(tokens, pos, in_definition)?;
            let else_branch = if is_word(tokens, *pos, "そうでなければ") {
                *pos += 1;
                Some(parse_branch(tokens, pos, in_definition)?)
            } else {
                None
            };
            expect_word(tokens, pos, "つぎに")?;
            exprs.push(Expr::IfElse {
                cond,
                then_branch,
                else_branch,
            });
            continue;
        }

        parse_atom_with_subscripts(tokens, pos, &mut exprs, in_definition)?;
    }
    Ok(exprs)
}

/// 1つの原子的な式（ワード呼び出し・数値・文字列/文字リテラル）を解析し、
/// 直後に隣接する`（）`添字アクセス糖衣構文（複数連鎖も可）があれば脱糖衣して
/// `out`へ追加する（例: `売り上げ（1）` → `WordCall("売り上げ")`,
/// `WordCall("の")`, `NumberLiteral(1)`, `WordCall("番目")`）。
fn parse_atom_with_subscripts(
    tokens: &[Token],
    pos: &mut usize,
    out: &mut Vec<Expr>,
    in_definition: bool,
) -> Result<(), ParseError> {
    out.push(parse_single_atom(tokens, pos, in_definition)?);

    while is_open_paren(tokens, *pos) {
        *pos += 1; // 「（」を読み飛ばす
        out.push(Expr::WordCall("の".to_string()));

        loop {
            if is_close_paren(tokens, *pos) {
                break;
            }
            if *pos >= tokens.len() {
                return Err(ParseError::new(
                    "添字アクセスが「）」で閉じられないまま入力が終了しました",
                    tokens,
                    *pos,
                ));
            }
            parse_atom_with_subscripts(tokens, pos, out, in_definition)?;
        }
        *pos += 1; // 「）」を読み飛ばす

        out.push(Expr::WordCall("番目".to_string()));
    }

    Ok(())
}

fn parse_loop_body(
    tokens: &[Token],
    pos: &mut usize,
    in_definition: bool,
) -> Result<Vec<Expr>, ParseError> {
    let mut exprs = Vec::new();
    loop {
        if is_word(tokens, *pos, "繰返") {
            *pos += 1;
            return Ok(exprs);
        }
        if *pos >= tokens.len() {
            return Err(ParseError::new(
                "ループが「繰り返し」「繰り返す」で閉じられないまま入力が終了しました",
                tokens,
                *pos,
            ));
        }

        if let Some(expr) = try_value_decl(tokens, pos, in_definition)? {
            exprs.push(expr);
            continue;
        }
        if let Some(expr) = try_assign(tokens, pos, &mut exprs) {
            exprs.push(expr);
            continue;
        }

        if is_word(tokens, *pos, "ここから") {
            *pos += 1;
            let body = parse_loop_body(tokens, pos, in_definition)?;
            exprs.push(Expr::InfiniteLoop { body });
            continue;
        }

        if is_word(tokens, *pos, "回数指定") {
            *pos += 1;
            let body = parse_loop_body(tokens, pos, in_definition)?;
            exprs.push(Expr::CountedLoop { body });
            continue;
        }

        if is_word(tokens, *pos, "ならば") {
            *pos += 1;
            let cond = std::mem::take(&mut exprs);
            let then_branch = parse_branch(tokens, pos, in_definition)?;
            let else_branch = if is_word(tokens, *pos, "そうでなければ") {
                *pos += 1;
                Some(parse_branch(tokens, pos, in_definition)?)
            } else {
                None
            };
            expect_word(tokens, pos, "つぎに")?;
            exprs.push(Expr::IfElse {
                cond,
                then_branch,
                else_branch,
            });
            continue;
        }

        parse_atom_with_subscripts(tokens, pos, &mut exprs, in_definition)?;
    }
}

fn parse_single_atom(
    tokens: &[Token],
    pos: &mut usize,
    in_definition: bool,
) -> Result<Expr, ParseError> {
    let kind = &tokens
        .get(*pos)
        .ok_or_else(|| ParseError::new("式が必要ですが入力が終了しました", tokens, *pos))?
        .kind;

    let expr = match kind {
        TokenKind::Word(w) if w == "。" => {
            return Err(ParseError::new(
                "「。」はここでは使用できません（区切り記号としての「。」を式の中で使うことはできません）",
                tokens,
                *pos,
            ));
        }
        TokenKind::Word(w) if w == "再帰" => {
            if !in_definition {
                return Err(ParseError::new(
                    "「再帰」は処理単語の定義内でのみ使用できます",
                    tokens,
                    *pos,
                ));
            }
            Expr::SelfRecurse
        }
        TokenKind::Word(w) if w == "打切" => Expr::Break,
        TokenKind::Word(w) if w == "繰返" => {
            return Err(ParseError::new(
                "「繰返」に対応するループの開始（「ここから」または「回数指定」）がありません",
                tokens,
                *pos,
            ));
        }
        TokenKind::Word(w) => Expr::WordCall(w.clone()),
        TokenKind::NumberLiteral(s) => Expr::NumberLiteral(parse_number_literal(s, tokens, *pos)?),
        TokenKind::StringLiteral(s) => Expr::StringLiteral(s.clone()),
        TokenKind::CharLiteral(c) => Expr::WordCall(format!("'{c}'")),
        TokenKind::OpenParen | TokenKind::CloseParen => {
            return Err(ParseError::new(
                "ここでは「（」「）」は使用できません（直前に添字アクセスの対象となる式がありません）",
                tokens,
                *pos,
            ));
        }
    };
    *pos += 1;
    Ok(expr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenize;

    fn parse_src(src: &str) -> Program {
        let tokens = tokenize(src).unwrap();
        parse(&tokens).unwrap_or_else(|e| panic!("parse failed for {src:?}: {e}"))
    }

    #[test]
    fn case1_simple_word_definition() {
        let program = parse_src("挨拶する とは、\n「こんにちは」を　表示すること。");
        assert_eq!(program.definitions.len(), 1);
        let def = &program.definitions[0];
        assert_eq!(def.locals, vec![]);
        assert_eq!(def.variables, Vec::<String>::new());
        assert_eq!(
            def.body,
            vec![
                Expr::StringLiteral("こんにちは".to_string()),
                Expr::WordCall("を".to_string()),
                Expr::WordCall("表示".to_string()),
            ]
        );
    }

    #[test]
    fn case2_local_word_with_honntai_towa() {
        let program = parse_src(
            "親処理 とは\n    子処理 とは\n        なにかする\n    本体 とは\n        子処理\nこと。",
        );
        assert_eq!(program.definitions.len(), 1);
        let parent = &program.definitions[0];
        assert_eq!(parent.name, "親処理");
        assert_eq!(parent.variables, Vec::<String>::new());
        assert_eq!(parent.body, vec![Expr::WordCall("子処理".to_string())]);
        assert_eq!(parent.locals.len(), 1);
        let child = &parent.locals[0];
        assert_eq!(child.name, "子処理");
        assert_eq!(child.locals, vec![]);
        assert_eq!(child.variables, Vec::<String>::new());
        assert_eq!(child.body, vec![Expr::WordCall("なにかする".to_string())]);
    }

    #[test]
    fn case3_if_else() {
        let program = parse_src(
            "判定する とは\n    雨降り？ ならば\n        傘を差す\n    そうでなければ\n        何もしない\n    つぎに\nこと。",
        );
        assert_eq!(program.definitions.len(), 1);
        let def = &program.definitions[0];
        assert_eq!(def.body.len(), 1);
        match &def.body[0] {
            Expr::IfElse {
                cond,
                then_branch,
                else_branch,
            } => {
                assert_eq!(cond, &vec![Expr::WordCall("雨降?".to_string())]);
                assert_eq!(then_branch, &vec![Expr::WordCall("傘差".to_string())]);
                assert_eq!(else_branch, &Some(vec![Expr::WordCall("何".to_string())]));
            }
            other => panic!("expected IfElse, got {other:?}"),
        }
    }

    #[test]
    fn case4_variable_declaration() {
        let program = parse_src("カウンター とは\n    X は 変数\n    0を　X に　いれる\nこと。");
        let def = &program.definitions[0];
        assert_eq!(def.variables, vec!["x".to_string()]);
        // ADR-0002: 「0を」は数値に空白なしで隣接しているため、「を」も
        // 含めてまるごと数値トークンへ畳み込まれ、独立したワードとしては
        // 現れない。
        assert_eq!(
            def.body,
            vec![
                Expr::NumberLiteral(0),
                Expr::WordCall("x".to_string()),
                Expr::WordCall("に".to_string()),
                Expr::WordCall("いれる".to_string()),
            ]
        );
    }

    #[test]
    fn case5_sibling_scope_violation_is_rejected_by_scope_check() {
        let program = parse_src(
            "親処理 とは\n    子処理１ とは\n        Y は 変数\n    子処理２ とは\n        Y に　1を　いれる\n    本体 とは\n        子処理１\n        子処理２\nこと。",
        );
        let result = check_scopes(&program);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.variable == "y"));
    }

    #[test]
    fn case6_subscript_desugaring() {
        let tokens = tokenize("売り上げ（1）").unwrap();
        let mut pos = 0usize;
        let mut out = Vec::new();
        parse_atom_with_subscripts(&tokens, &mut pos, &mut out, false).unwrap();
        assert_eq!(
            out,
            vec![
                Expr::WordCall("売上".to_string()),
                Expr::WordCall("の".to_string()),
                Expr::NumberLiteral(1),
                Expr::WordCall("番目".to_string()),
            ]
        );
    }

    #[test]
    fn chained_subscript_desugaring() {
        let tokens = tokenize("ダンジョンマップ（X軸座標）（Y座標）").unwrap();
        let mut pos = 0usize;
        let mut out = Vec::new();
        parse_atom_with_subscripts(&tokens, &mut pos, &mut out, false).unwrap();
        assert_eq!(
            out,
            vec![
                Expr::WordCall("ダンジョンマップ".to_string()),
                Expr::WordCall("の".to_string()),
                Expr::WordCall("x軸座標".to_string()),
                Expr::WordCall("番目".to_string()),
                Expr::WordCall("の".to_string()),
                Expr::WordCall("y座標".to_string()),
                Expr::WordCall("番目".to_string()),
            ]
        );
    }

    #[test]
    fn no_locals_variable_is_visible_within_own_definition() {
        let program = parse_src("カウンター とは\n    X は 変数\n    0を　X に　いれる\nこと。");
        assert_eq!(check_scopes(&program), Ok(()));
    }

    #[test]
    fn local_can_see_parent_variable() {
        let program = parse_src(
            "親処理 とは\n    X は 変数\n    子処理 とは\n        X に　1を　いれる\n    本体 とは\n        子処理\nこと。",
        );
        assert_eq!(check_scopes(&program), Ok(()));
    }

    #[test]
    fn adr0030_alias_decl_is_recognized() {
        // 「も」「の」はいずれも構造キーワードのため、直前にスペースが必要
        // （ADR-0015の「は」と同様、スペースがないと送り仮名として吸収され消える）。
        let tokens = tokenize("交換 も 取替 の 別名。").unwrap();
        let mut pos = 0usize;
        match parse_top_level_item(&tokens, &mut pos).unwrap() {
            TopLevelItem::Expr(exprs) => {
                assert_eq!(
                    exprs,
                    vec![Expr::AliasDecl {
                        new_name: "交換".to_string(),
                        existing_name: "取替".to_string(),
                    }]
                );
            }
            other => panic!("expected Expr, got {other:?}"),
        }
        assert_eq!(pos, tokens.len());
    }

    #[test]
    fn unclosed_definition_is_a_parse_error() {
        let tokens = tokenize("挨拶する とは\n「こんにちは」を　表示する").unwrap();
        assert!(parse(&tokens).is_err());
    }
}
