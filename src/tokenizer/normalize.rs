//! 全角/半角・大文字/小文字の正規化。
//!
//! トークンの比較・辞書引きにおいては、以下を同一視する:
//! - 半角カタカナ ⇔ 全角カタカナ
//! - 半角英数字・記号 ⇔ 全角英数字・記号
//! - 英字の大文字 ⇔ 小文字
//!
//! 正規化の方向は「半角カタカナは全角へ」「全角英数記号は半角へ」
//! 「英字は小文字へ」に統一する。

use super::kana_table::{halfwidth_katakana_variant, SEMI_VOICED_MARK, VOICED_MARK};

/// 全角英数字・記号（U+FF01〜U+FF5E）を対応する半角文字に変換する。
/// 対象外の文字は `None` を返す。
fn fullwidth_ascii_to_halfwidth(c: char) -> Option<char> {
    let u = c as u32;
    if (0xFF01..=0xFF5E).contains(&u) {
        char::from_u32(u - 0xFEE0)
    } else {
        None
    }
}

/// 文字列を正規化する（幅・大文字小文字のみ。送り仮名除去は含まない）。
pub fn normalize_width_and_case(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if let Some((base, voiced, semivoiced)) = halfwidth_katakana_variant(c) {
            let next = chars.get(i + 1).copied();
            if let (Some(VOICED_MARK), Some(v)) = (next, voiced) {
                out.push(v);
                i += 2;
                continue;
            }
            if let (Some(SEMI_VOICED_MARK), Some(sv)) = (next, semivoiced) {
                out.push(sv);
                i += 2;
                continue;
            }
            out.push(base);
            i += 1;
            continue;
        }

        if c == '\u{3000}' {
            out.push(' ');
            i += 1;
            continue;
        }

        if let Some(half) = fullwidth_ascii_to_halfwidth(c) {
            for lc in half.to_lowercase() {
                out.push(lc);
            }
            i += 1;
            continue;
        }

        for lc in c.to_lowercase() {
            out.push(lc);
        }
        i += 1;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_halfwidth_katakana_to_fullwidth() {
        assert_eq!(normalize_width_and_case("ｱｲｳｴｵ"), "アイウエオ");
    }

    #[test]
    fn combines_dakuten_and_handakuten() {
        assert_eq!(normalize_width_and_case("ﾊﾞﾋﾟｶﾞ"), "バピガ");
    }

    #[test]
    fn converts_fullwidth_ascii_to_halfwidth() {
        assert_eq!(normalize_width_and_case("ＡＢＣ１２３"), "abc123");
    }

    #[test]
    fn lowercases_ascii_letters() {
        assert_eq!(normalize_width_and_case("Hello"), "hello");
    }

    #[test]
    fn leaves_kanji_and_hiragana_untouched() {
        assert_eq!(normalize_width_and_case("表示する"), "表示する");
    }
}
