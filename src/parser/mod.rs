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

pub use ast::{Definition, Expr, Program};
pub use error::ParseError;
pub use scope::{check_scopes, ScopeError};

use crate::tokenizer::{Token, TokenKind};

/// トークン列を構文解析し、[`Program`]を返す。
pub fn parse(tokens: &[Token]) -> Result<Program, ParseError> {
    let mut pos = 0usize;
    let mut definitions = Vec::new();
    while pos < tokens.len() {
        definitions.push(parse_definition(tokens, &mut pos, false)?);
    }
    Ok(Program { definitions })
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
    let name = expect_any_word(tokens, pos)?;
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

        if is_word(tokens, *pos, "ならば") {
            *pos += 1;
            let cond = std::mem::take(&mut body);
            let then_branch = parse_branch(tokens, pos)?;
            let else_branch = if is_word(tokens, *pos, "そうでなければ") {
                *pos += 1;
                Some(parse_branch(tokens, pos)?)
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

        parse_atom_with_subscripts(tokens, pos, &mut body)?;
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
fn parse_branch(tokens: &[Token], pos: &mut usize) -> Result<Vec<Expr>, ParseError> {
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

        if is_word(tokens, *pos, "ならば") {
            *pos += 1;
            let cond = std::mem::take(&mut exprs);
            let then_branch = parse_branch(tokens, pos)?;
            let else_branch = if is_word(tokens, *pos, "そうでなければ") {
                *pos += 1;
                Some(parse_branch(tokens, pos)?)
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

        parse_atom_with_subscripts(tokens, pos, &mut exprs)?;
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
) -> Result<(), ParseError> {
    out.push(parse_single_atom(tokens, pos)?);

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
            parse_atom_with_subscripts(tokens, pos, out)?;
        }
        *pos += 1; // 「）」を読み飛ばす

        out.push(Expr::WordCall("番目".to_string()));
    }

    Ok(())
}

fn parse_single_atom(tokens: &[Token], pos: &mut usize) -> Result<Expr, ParseError> {
    let kind = &tokens
        .get(*pos)
        .ok_or_else(|| ParseError::new("式が必要ですが入力が終了しました", tokens, *pos))?
        .kind;

    let expr = match kind {
        TokenKind::Word(w) => Expr::WordCall(w.clone()),
        TokenKind::NumberLiteral(s) => Expr::NumberLiteral(parse_number_literal(s, tokens, *pos)?),
        TokenKind::StringLiteral(s) => Expr::WordCall(format!("「{s}」")),
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
        let tokens = tokenize(src);
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
                Expr::WordCall("「こんにちは」".to_string()),
                Expr::WordCall("を".to_string()),
                Expr::WordCall("表示".to_string()),
            ]
        );
    }

    #[test]
    fn case2_local_word_with_honntai_towa() {
        let program = parse_src(
            "親処理とは\n    子処理とは\n        なにかする\n    本体とは\n        子処理\nこと。",
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
            "判定するとは\n    雨降り？ ならば\n        傘を差す\n    そうでなければ\n        何もしない\n    つぎに\nこと。",
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
                assert_eq!(cond, &vec![Expr::WordCall("雨降り?".to_string())]);
                assert_eq!(then_branch, &vec![Expr::WordCall("傘を差".to_string())]);
                assert_eq!(else_branch, &Some(vec![Expr::WordCall("何".to_string())]));
            }
            other => panic!("expected IfElse, got {other:?}"),
        }
    }

    #[test]
    fn case4_variable_declaration() {
        let program = parse_src("カウンターとは\n    Xは 変数\n    0を　X に　いれる\nこと。");
        let def = &program.definitions[0];
        assert_eq!(def.variables, vec!["x".to_string()]);
        assert_eq!(
            def.body,
            vec![
                Expr::NumberLiteral(0),
                Expr::WordCall("を".to_string()),
                Expr::WordCall("x".to_string()),
                Expr::WordCall("に".to_string()),
                Expr::WordCall("いれる".to_string()),
            ]
        );
    }

    #[test]
    fn case5_sibling_scope_violation_is_rejected_by_scope_check() {
        let program = parse_src(
            "親処理とは\n    子処理１とは\n        Yは 変数\n    子処理２とは\n        Y に　1を　いれる\n    本体とは\n        子処理１\n        子処理２\nこと。",
        );
        let result = check_scopes(&program);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors.iter().any(|e| e.variable == "y"));
    }

    #[test]
    fn case6_subscript_desugaring() {
        let tokens = tokenize("売り上げ（1）");
        let mut pos = 0usize;
        let mut out = Vec::new();
        parse_atom_with_subscripts(&tokens, &mut pos, &mut out).unwrap();
        assert_eq!(
            out,
            vec![
                Expr::WordCall("売り上".to_string()),
                Expr::WordCall("の".to_string()),
                Expr::NumberLiteral(1),
                Expr::WordCall("番目".to_string()),
            ]
        );
    }

    #[test]
    fn chained_subscript_desugaring() {
        let tokens = tokenize("ダンジョンマップ（X軸座標）（Y座標）");
        let mut pos = 0usize;
        let mut out = Vec::new();
        parse_atom_with_subscripts(&tokens, &mut pos, &mut out).unwrap();
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
        let program = parse_src("カウンターとは\n    Xは 変数\n    0を　X に　いれる\nこと。");
        assert_eq!(check_scopes(&program), Ok(()));
    }

    #[test]
    fn local_can_see_parent_variable() {
        let program = parse_src(
            "親処理とは\n    Xは 変数\n    子処理とは\n        X に　1を　いれる\n    本体とは\n        子処理\nこと。",
        );
        assert_eq!(check_scopes(&program), Ok(()));
    }

    #[test]
    fn unclosed_definition_is_a_parse_error() {
        let tokens = tokenize("挨拶する とは\n「こんにちは」を　表示する");
        assert!(parse(&tokens).is_err());
    }
}
