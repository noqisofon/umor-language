//! 送り仮名の正規化ロジック。
//!
//! Umorの字句解析における中核ルール:
//! 漢字部分を語幹として抽出し、活用による変化部分（送り仮名）をすべて捨てる。
//! 語尾パターン（「反応する」→「反応」）だけでなく、漢字の間にひらがなが
//! 挟まる語中パターン（「繰り返し」「繰り返す」→「繰返」）も含む。
//!
//! ただしトークンが全てひらがな（＋長音記号「ー」）で構成されている場合は
//! 一切削らない（例: 「ならば」「つぎに」「ここから」）。
//! 先頭のひらがなは削らない（例: 「ご案内する」→「ご案内」）。

/// 文字がひらがなかどうかを判定する。
///
/// Unicode範囲 U+3041〜U+3096（ぁ〜ゖ）および U+309D〜U+309F（ゝゞゟ）を
/// ひらがなとみなす。長音記号「ー」（U+30FC）はひらがなに含めない。
pub fn is_hiragana(c: char) -> bool {
    let u = c as u32;
    (0x3041..=0x3096).contains(&u) || (0x309D..=0x309F).contains(&u)
}

/// トークンの送り仮名を除去して語幹を抽出する。
///
/// 全ひらがな（または長音記号「ー」との組み合わせ）トークンは正規化対象外として
/// そのまま返す。
///
/// それ以外の場合、最初の非ひらがな（・非長音記号）文字より手前のひらがなは保持し
/// （例: 「ご案内する」→「ご案内」）、最初の非ひらがな文字以降はすべてのひらがなを
/// 削除する（例: 「繰り返し」「繰り返す」→「繰返」、「打ち切り」→「打切」）。
pub fn normalize_okurigana(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();

    if chars.iter().all(|&c| is_hiragana(c) || c == 'ー') {
        return token.to_string();
    }

    let first_non_hiragana_idx = match chars
        .iter()
        .position(|&c| !is_hiragana(c) && c != 'ー')
    {
        Some(idx) => idx,
        None => return token.to_string(),
    };

    let mut result = String::new();
    for &c in &chars[..first_non_hiragana_idx] {
        result.push(c);
    }
    for &c in &chars[first_non_hiragana_idx..] {
        if !is_hiragana(c) {
            result.push(c);
        }
    }

    if result.is_empty() {
        token.to_string()
    } else {
        result
    }
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
    fn strips_infix_and_suffix_hiragana_for_compound_words() {
        assert_eq!(normalize_okurigana("繰り返し"), "繰返");
        assert_eq!(normalize_okurigana("繰り返す"), "繰返");
        assert_eq!(normalize_okurigana("打ち切り"), "打切");
        assert_eq!(normalize_okurigana("打ち切る"), "打切");
        assert_eq!(normalize_okurigana("回数指定し"), "回数指定");
    }

    #[test]
    fn keeps_all_hiragana_tokens_untouched() {
        assert_eq!(normalize_okurigana("ならば"), "ならば");
        assert_eq!(normalize_okurigana("つぎに"), "つぎに");
        assert_eq!(normalize_okurigana("さもなければ"), "さもなければ");
        assert_eq!(normalize_okurigana("ここから"), "ここから");
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
