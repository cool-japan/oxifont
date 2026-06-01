//! BCP-47 ↔ Windows LCID mapping for locale-aware font name resolution.
//!
//! The `name` table in OpenType fonts uses Windows LCID values to identify
//! the language of each name record.  This module provides a static lookup
//! table that maps a lower-cased BCP-47 locale tag (e.g. `"ja-jp"`) to its
//! corresponding Windows LCID (e.g. `0x0411`).

/// Static BCP-47 → Windows LCID mapping (lower-case keys).
static BCP47_TO_LCID: &[(&str, u16)] = &[
    ("en-us", 0x0409),
    ("en-gb", 0x0809),
    ("en-au", 0x0C09),
    ("en-ca", 0x1009),
    ("ja-jp", 0x0411),
    ("ko-kr", 0x0412),
    ("zh-cn", 0x0804),
    ("zh-tw", 0x0404),
    ("zh-hk", 0x0C04),
    ("de-de", 0x0407),
    ("de-at", 0x0C07),
    ("de-ch", 0x0807),
    ("fr-fr", 0x040C),
    ("fr-be", 0x080C),
    ("fr-ch", 0x100C),
    ("fr-ca", 0x0C0C),
    ("es-es", 0x0C0A),
    ("es-mx", 0x080A),
    ("es-ar", 0x2C0A),
    ("it-it", 0x0410),
    ("pt-pt", 0x0816),
    ("pt-br", 0x0416),
    ("ru-ru", 0x0419),
    ("ar-sa", 0x0401),
    ("pl-pl", 0x0415),
    ("nl-nl", 0x0413),
    ("nl-be", 0x0813),
    ("sv-se", 0x041D),
    ("da-dk", 0x0406),
    ("fi-fi", 0x040B),
    ("nb-no", 0x0414),
    ("nn-no", 0x0814),
    ("cs-cz", 0x0405),
    ("hu-hu", 0x040E),
    ("ro-ro", 0x0418),
    ("sk-sk", 0x041B),
    ("uk-ua", 0x0422),
    ("bg-bg", 0x0402),
    ("hr-hr", 0x041A),
    ("lt-lt", 0x0427),
    ("lv-lv", 0x0426),
    ("et-ee", 0x0425),
    ("tr-tr", 0x041F),
    ("vi-vn", 0x042A),
    ("th-th", 0x041E),
    ("id-id", 0x0421),
    ("ms-my", 0x043E),
    ("el-gr", 0x0408),
    ("he-il", 0x040D),
    ("fa-ir", 0x0429),
    ("ur-pk", 0x0420),
    ("hi-in", 0x0439),
    ("bn-in", 0x0445),
    ("ta-in", 0x0449),
    ("te-in", 0x044A),
    ("mr-in", 0x044E),
    ("kn-in", 0x044B),
    ("ml-in", 0x044C),
    ("sr-latn-rs", 0x241A),
    ("sr-cyrl-rs", 0x281A),
    ("ca-es", 0x0403),
    ("gl-es", 0x0456),
    ("eu-es", 0x042D),
];

/// Looks up the Windows LCID for a BCP-47 locale tag.
///
/// The comparison is **case-insensitive** (input is normalised to lowercase).
///
/// # Examples
/// ```
/// use oxifont_db::locale::bcp47_to_lcid;
/// assert_eq!(bcp47_to_lcid("ja-JP"), Some(0x0411));
/// assert_eq!(bcp47_to_lcid("en-US"), Some(0x0409));
/// assert_eq!(bcp47_to_lcid("xx-ZZ"), None);
/// ```
pub fn bcp47_to_lcid(bcp47: &str) -> Option<u16> {
    let lower = bcp47.to_lowercase();
    BCP47_TO_LCID
        .iter()
        .find(|(k, _)| *k == lower.as_str())
        .map(|(_, v)| *v)
}
