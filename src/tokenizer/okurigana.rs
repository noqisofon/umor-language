//! 送り仮名の正規化ロジック。
//!
//! Umorの字句解析における中核ルール:
//! 正規化済みトークンの末尾から、ひらがな文字が連続する限りこれを削る。
//! ただしトークンが全てひらがな（＋長音記号「ー」）で構成されている場合は
//! 一切削らない。全部消えてしまう場合も削らない。

/// 文字がひらがなかどうかを判定する。
///
/// Unicode範囲 U+3041〜U+3096（ぁ〜ゖ）および U+309D〜U+309F（ゝゞゟ）を
/// ひらがなとみなす。長音記号「ー」（U+30FC）はひらがなに含めない。
pub fn is_hiragana(c: char) -> bool {
    let u = c as u32;
    (0x3041..=0x3096).contains(&u) || (0x309D..=0x309F).contains(&u)
}

/// トークン末尾の送り仮名（ひらがな連続）を除去する。
///
/// トークンが全てひらがな（または長音記号「ー」との組み合わせ）で
/// 構成されている場合は、削らずそのまま返す
/// （例: 「ならば」「つぎに」は保持される）。
///
/// それ以外の場合、末尾からひらがなが続く限り削る
/// （例: 「反応する」→「反応」、「表示し」→「表示」）。
/// 先頭のひらがなは削らない（例: 「ご案内する」→「ご案内」）。
pub fn normalize_okurigana(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();

    if chars.iter().all(|&c| is_hiragana(c) || c == 'ー') {
        return token.to_string();
    }

    let mut end = chars.len();
    while end > 0 && is_hiragana(chars[end - 1]) {
        end -= 1;
    }

    if end == 0 {
        return token.to_string();
    }

    chars[..end].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_trailing_hiragana_after_kanji() {
        assert_eq!(normalize_okurigana("反応し"), "反応");
        assert_eq!(normalize_okurigana("反応する"), "反応");
        assert_eq!(normalize_okurigana("反応させる"), "反応");
        assert_eq!(normalize_okurigana("表示する"), "表示");
        assert_eq!(normalize_okurigana("表示し"), "表示");
    }

    #[test]
    fn keeps_all_hiragana_tokens_untouched() {
        assert_eq!(normalize_okurigana("ならば"), "ならば");
        assert_eq!(normalize_okurigana("つぎに"), "つぎに");
        assert_eq!(normalize_okurigana("さもなければ"), "さもなければ");
    }

    #[test]
    fn keeps_leading_hiragana() {
        assert_eq!(normalize_okurigana("ご案内する"), "ご案内");
    }

    #[test]
    fn does_not_strip_choonpu() {
        assert_eq!(normalize_okurigana("そーする"), "そーする");
    }

    #[test]
    fn token_without_trailing_hiragana_is_unchanged() {
        assert_eq!(normalize_okurigana("表示"), "表示");
        assert_eq!(normalize_okurigana("円"), "円");
    }
}
