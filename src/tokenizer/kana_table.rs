//! 半角カタカナ → 全角カタカナ変換テーブル（JIS X 0201 準拠）。
//!
//! 各エントリは (半角文字, 全角基本形, 濁点付き全角形, 半濁点付き全角形) を表す。
//! 濁点（ﾞ, U+FF9E）・半濁点（ﾟ, U+FF9F）が後続する場合は、
//! 対応する濁音・半濁音の全角文字に変換する。

/// 半角カタカナ1文字に対応する変換情報を返す。
///
/// 戻り値は (基本形, 濁点付き形, 半濁点付き形) のタプル。
/// 濁点・半濁点の変化形を持たない文字は `None` を返す。
pub fn halfwidth_katakana_variant(c: char) -> Option<(char, Option<char>, Option<char>)> {
    let entry = match c {
        '｡' => ('。', None, None),
        '｢' => ('「', None, None),
        '｣' => ('」', None, None),
        '､' => ('、', None, None),
        '･' => ('・', None, None),
        'ｦ' => ('ヲ', None, None),
        'ｧ' => ('ァ', None, None),
        'ｨ' => ('ィ', None, None),
        'ｩ' => ('ゥ', None, None),
        'ｪ' => ('ェ', None, None),
        'ｫ' => ('ォ', None, None),
        'ｬ' => ('ャ', None, None),
        'ｭ' => ('ュ', None, None),
        'ｮ' => ('ョ', None, None),
        'ｯ' => ('ッ', None, None),
        'ｰ' => ('ー', None, None),
        'ｱ' => ('ア', None, None),
        'ｲ' => ('イ', None, None),
        'ｳ' => ('ウ', Some('ヴ'), None),
        'ｴ' => ('エ', None, None),
        'ｵ' => ('オ', None, None),
        'ｶ' => ('カ', Some('ガ'), None),
        'ｷ' => ('キ', Some('ギ'), None),
        'ｸ' => ('ク', Some('グ'), None),
        'ｹ' => ('ケ', Some('ゲ'), None),
        'ｺ' => ('コ', Some('ゴ'), None),
        'ｻ' => ('サ', Some('ザ'), None),
        'ｼ' => ('シ', Some('ジ'), None),
        'ｽ' => ('ス', Some('ズ'), None),
        'ｾ' => ('セ', Some('ゼ'), None),
        'ｿ' => ('ソ', Some('ゾ'), None),
        'ﾀ' => ('タ', Some('ダ'), None),
        'ﾁ' => ('チ', Some('ヂ'), None),
        'ﾂ' => ('ツ', Some('ヅ'), None),
        'ﾃ' => ('テ', Some('デ'), None),
        'ﾄ' => ('ト', Some('ド'), None),
        'ﾅ' => ('ナ', None, None),
        'ﾆ' => ('ニ', None, None),
        'ﾇ' => ('ヌ', None, None),
        'ﾈ' => ('ネ', None, None),
        'ﾉ' => ('ノ', None, None),
        'ﾊ' => ('ハ', Some('バ'), Some('パ')),
        'ﾋ' => ('ヒ', Some('ビ'), Some('ピ')),
        'ﾌ' => ('フ', Some('ブ'), Some('プ')),
        'ﾍ' => ('ヘ', Some('ベ'), Some('ペ')),
        'ﾎ' => ('ホ', Some('ボ'), Some('ポ')),
        'ﾏ' => ('マ', None, None),
        'ﾐ' => ('ミ', None, None),
        'ﾑ' => ('ム', None, None),
        'ﾒ' => ('メ', None, None),
        'ﾓ' => ('モ', None, None),
        'ﾔ' => ('ヤ', None, None),
        'ﾕ' => ('ユ', None, None),
        'ﾖ' => ('ヨ', None, None),
        'ﾗ' => ('ラ', None, None),
        'ﾘ' => ('リ', None, None),
        'ﾙ' => ('ル', None, None),
        'ﾚ' => ('レ', None, None),
        'ﾛ' => ('ロ', None, None),
        'ﾜ' => ('ワ', None, None),
        'ﾝ' => ('ン', None, None),
        _ => return None,
    };
    Some(entry)
}

/// 濁点結合文字（U+FF9E）。
pub const VOICED_MARK: char = '\u{FF9E}';
/// 半濁点結合文字（U+FF9F）。
pub const SEMI_VOICED_MARK: char = '\u{FF9F}';

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_plain_kana() {
        assert_eq!(halfwidth_katakana_variant('ｱ'), Some(('ア', None, None)));
    }

    #[test]
    fn maps_voiced_and_semivoiced_variants() {
        assert_eq!(
            halfwidth_katakana_variant('ﾊ'),
            Some(('ハ', Some('バ'), Some('パ')))
        );
        assert_eq!(halfwidth_katakana_variant('ｶ'), Some(('カ', Some('ガ'), None)));
    }

    #[test]
    fn non_halfwidth_katakana_returns_none() {
        assert_eq!(halfwidth_katakana_variant('あ'), None);
        assert_eq!(halfwidth_katakana_variant('A'), None);
    }
}
