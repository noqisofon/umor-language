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
        Expr::NumberLiteral(_) => {}
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
