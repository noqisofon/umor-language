//! 変数スコープの妥当性検証（パースとは別パス）。
//!
//! - 局所処理単語は、親（およびその祖先）の`variables`を参照できる。
//! - 兄弟の局所処理単語同士は、互いの`variables`にアクセスできない。
//!
//! AST上では変数の読み取り・代入も普通の`Expr::WordCall`と区別されないため、
//! 「トップレベル定義1つとその配下（locals）全体」の中で宣言された変数名の
//! 集合を求め、各`WordCall`がその集合に含まれる名前でありながら、
//! 出現位置から可視でない場合にスコープ違反として報告する。

use super::ast::{Definition, Expr, Program};
use std::collections::HashSet;
use std::fmt;

/// スコープ違反エラー。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeError {
    /// 違反対象の変数名。
    pub variable: String,
    /// 違反が検出されたワード定義の名前。
    pub definition_name: String,
    pub message: String,
}

impl fmt::Display for ScopeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "スコープエラー: {}", self.message)
    }
}

impl std::error::Error for ScopeError {}

/// プログラム全体の変数スコープを検証する。
///
/// 違反が1件もなければ`Ok(())`、あれば検出した全ての`ScopeError`を返す。
pub fn check_scopes(program: &Program) -> Result<(), Vec<ScopeError>> {
    let mut errors = Vec::new();
    for def in &program.definitions {
        let mut declared_in_tree = HashSet::new();
        collect_variable_names(def, &mut declared_in_tree);
        check_definition(def, &declared_in_tree, &[], &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn collect_variable_names(def: &Definition, out: &mut HashSet<String>) {
    out.extend(def.variables.iter().cloned());
    for local in &def.locals {
        collect_variable_names(local, out);
    }
}

fn check_definition(
    def: &Definition,
    declared_in_tree: &HashSet<String>,
    ancestor_variables: &[String],
    errors: &mut Vec<ScopeError>,
) {
    let mut visible: Vec<String> = ancestor_variables.to_vec();
    visible.extend(def.variables.iter().cloned());

    for expr in &def.body {
        check_expr(expr, def, declared_in_tree, &visible, errors);
    }
    for local in &def.locals {
        check_definition(local, declared_in_tree, &visible, errors);
    }
}

fn check_expr(
    expr: &Expr,
    def: &Definition,
    declared_in_tree: &HashSet<String>,
    visible: &[String],
    errors: &mut Vec<ScopeError>,
) {
    match expr {
        Expr::WordCall(name) => {
            if declared_in_tree.contains(name) && !visible.iter().any(|v| v == name) {
                errors.push(ScopeError {
                    variable: name.clone(),
                    definition_name: def.name.clone(),
                    message: format!(
                        "変数「{name}」は「{}」の中では見えません（宣言されたスコープの外です）",
                        def.name
                    ),
                });
            }
        }
        // ADR-0026: トップレベルの変数宣言。定義本体の中には現れない
        // （パーサーはトップレベルの1要素としてのみ生成する）ため、
        // スコープ検証の対象にはならない。
        Expr::VariableDecl(_) => {}
        // ADR-0030: トップレベルのエイリアス宣言。VariableDeclと同様、
        // 定義本体の中には現れないため、スコープ検証の対象にはならない。
        Expr::AliasDecl { .. } => {}
        // ADR-0020/ADR-0031: 可変値・定数値は`変数`とは別のスコープ機構
        // （Interpreterの`value_scope_chain`）で管理されるため、`変数`専用の
        // `declared_in_tree`／`visible`によるスコープチェックの対象には
        // しない（未配線のまま、ADR-0013の宿題）。内部の式は再帰的に
        // チェックする。
        Expr::ValueDecl { init_expr, .. } => {
            for e in init_expr {
                check_expr(e, def, declared_in_tree, visible, errors);
            }
        }
        Expr::Assign { value_expr, .. } => {
            for e in value_expr {
                check_expr(e, def, declared_in_tree, visible, errors);
            }
        }
        Expr::NumberLiteral(_) => {}
        Expr::StringLiteral(_) => {}
        Expr::SelfRecurse => {}
        Expr::Break => {}
        Expr::InfiniteLoop { body } => {
            for e in body {
                check_expr(e, def, declared_in_tree, visible, errors);
            }
        }
        Expr::CountedLoop { body } => {
            let mut inner_visible = visible.to_vec();
            if !inner_visible.iter().any(|v| v == "回数") {
                inner_visible.push("回数".to_string());
            }
            for e in body {
                check_expr(e, def, declared_in_tree, &inner_visible, errors);
            }
        }
        Expr::IfElse {
            cond,
            then_branch,
            else_branch,
        } => {
            for e in cond
                .iter()
                .chain(then_branch.iter())
                .chain(else_branch.iter().flatten())
            {
                check_expr(e, def, declared_in_tree, visible, errors);
            }
        }
    }
}
