use std::borrow::Cow;
use std::io;
use std::path::Path;

use chardetng::EncodingDetector;
use encoding_rs::{
    BIG5, EUC_JP, EUC_KR, Encoding, GBK, ISO_2022_JP, SHIFT_JIS, UTF_16BE, UTF_16LE,
    WINDOWS_1252,
};

use crate::i18n::UiLanguage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileEncodingPreference {
    Auto,
    Utf8,
    Utf16Le,
    Utf16Be,
    ShiftJis,
    EucJp,
    Iso2022Jp,
    Gbk,
    Big5,
    EucKr,
    Windows1252,
}

impl Default for FileEncodingPreference {
    fn default() -> Self {
        Self::Auto
    }
}

impl FileEncodingPreference {
    pub const ALL: [Self; 11] = [
        Self::Auto,
        Self::Utf8,
        Self::Utf16Le,
        Self::Utf16Be,
        Self::ShiftJis,
        Self::EucJp,
        Self::Iso2022Jp,
        Self::Gbk,
        Self::Big5,
        Self::EucKr,
        Self::Windows1252,
    ];

    pub fn display_label(self, ui_lang: UiLanguage) -> &'static str {
        match (ui_lang, self) {
            (UiLanguage::Japanese, Self::Auto) => "自動判定",
            (UiLanguage::Japanese, Self::Utf8) => "UTF-8",
            (UiLanguage::Japanese, Self::Utf16Le) => "UTF-16 LE",
            (UiLanguage::Japanese, Self::Utf16Be) => "UTF-16 BE",
            (UiLanguage::Japanese, Self::ShiftJis) => "Shift_JIS (CP932)",
            (UiLanguage::Japanese, Self::EucJp) => "EUC-JP",
            (UiLanguage::Japanese, Self::Iso2022Jp) => "JIS (ISO-2022-JP)",
            (UiLanguage::Japanese, Self::Gbk) => "GBK / GB18030",
            (UiLanguage::Japanese, Self::Big5) => "Big5",
            (UiLanguage::Japanese, Self::EucKr) => "EUC-KR",
            (UiLanguage::Japanese, Self::Windows1252) => "Latin1 / Windows-1252",
            (UiLanguage::English, Self::Auto) => "Auto detect",
            (UiLanguage::English, Self::Utf8) => "UTF-8",
            (UiLanguage::English, Self::Utf16Le) => "UTF-16 LE",
            (UiLanguage::English, Self::Utf16Be) => "UTF-16 BE",
            (UiLanguage::English, Self::ShiftJis) => "Shift_JIS (CP932)",
            (UiLanguage::English, Self::EucJp) => "EUC-JP",
            (UiLanguage::English, Self::Iso2022Jp) => "JIS (ISO-2022-JP)",
            (UiLanguage::English, Self::Gbk) => "GBK / GB18030",
            (UiLanguage::English, Self::Big5) => "Big5",
            (UiLanguage::English, Self::EucKr) => "EUC-KR",
            (UiLanguage::English, Self::Windows1252) => "Latin1 / Windows-1252",
        }
    }

    fn to_file_encoding(self) -> Option<FileEncoding> {
        match self {
            Self::Auto => None,
            Self::Utf8 => Some(FileEncoding::Utf8),
            Self::Utf16Le => Some(FileEncoding::Utf16Le),
            Self::Utf16Be => Some(FileEncoding::Utf16Be),
            Self::ShiftJis => Some(FileEncoding::ShiftJis),
            Self::EucJp => Some(FileEncoding::EucJp),
            Self::Iso2022Jp => Some(FileEncoding::Iso2022Jp),
            Self::Gbk => Some(FileEncoding::Gbk),
            Self::Big5 => Some(FileEncoding::Big5),
            Self::EucKr => Some(FileEncoding::EucKr),
            Self::Windows1252 => Some(FileEncoding::Windows1252),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileEncoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    ShiftJis,
    EucJp,
    Iso2022Jp,
    Gbk,
    Big5,
    EucKr,
    Windows1252,
    Detected(String),
}

#[derive(Debug, Clone)]
pub struct DecodedText {
    pub text: String,
    pub encoding: FileEncoding,
}

pub fn read_text_file(path: &Path, preference: FileEncodingPreference) -> io::Result<DecodedText> {
    let bytes = std::fs::read(path)?;
    decode_text_bytes(&bytes, preference)
}

pub fn read_text_file_as(path: &Path, encoding: FileEncoding) -> io::Result<String> {
    let bytes = std::fs::read(path)?;
    decode_bytes_as(&bytes, encoding)
}

fn decode_text_bytes(bytes: &[u8], preference: FileEncodingPreference) -> io::Result<DecodedText> {
    let decoded = match preference {
        FileEncodingPreference::Auto => decode_auto(bytes),
        pref => decode_manual(bytes, pref),
    }?;

    if is_probably_binary_text(&decoded.text) {
        return Err(invalid_data("binary file"));
    }

    Ok(decoded)
}

fn decode_auto(bytes: &[u8]) -> io::Result<DecodedText> {
    if bytes.is_empty() {
        return Ok(DecodedText {
            text: String::new(),
            encoding: FileEncoding::Utf8,
        });
    }

    if let Some(decoded) = decode_bom_prefixed(bytes)? {
        return Ok(decoded);
    }

    if let Ok(text) = decode_utf8(bytes) {
        return Ok(DecodedText {
            text,
            encoding: FileEncoding::Utf8,
        });
    }

    if let Some(encoding) = guess_utf16_without_bom(bytes) {
        let text = decode_with_encoding(bytes, encoding)?;
        return Ok(DecodedText {
            text,
            encoding: FileEncoding::from_encoding(encoding),
        });
    }

    let mut detector = EncodingDetector::new();
    detector.feed(bytes, true);

    let encoding = detector.guess(None, true);
    let text = decode_with_encoding(bytes, encoding)?;

    Ok(DecodedText {
        text,
        encoding: FileEncoding::from_encoding(encoding),
    })
}

fn decode_manual(bytes: &[u8], preference: FileEncodingPreference) -> io::Result<DecodedText> {
    let encoding = preference
        .to_file_encoding()
        .ok_or_else(|| invalid_data("manual encoding required"))?;
    let text = decode_bytes_as(bytes, encoding.clone())?;
    Ok(DecodedText { text, encoding })
}

fn decode_bytes_as(bytes: &[u8], encoding: FileEncoding) -> io::Result<String> {
    let resolved = encoding
        .encoding()
        .ok_or_else(|| invalid_data("unknown encoding label"))?;
    decode_with_encoding(bytes, resolved)
}

fn decode_utf8(bytes: &[u8]) -> io::Result<String> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8(bytes.to_vec()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn decode_with_encoding(bytes: &[u8], encoding: &'static Encoding) -> io::Result<String> {
    let (decoded, _, had_errors) = encoding.decode(bytes);
    if had_errors {
        return Err(invalid_data("invalid text data for detected encoding"));
    }

    let mut text = decoded.into_owned();
    if let Some(stripped) = text.strip_prefix('\u{feff}') {
        text = stripped.to_string();
    }
    Ok(text)
}

impl FileEncoding {
    pub fn display_label(&self) -> Cow<'static, str> {
        match self {
            Self::Utf8 => Cow::Borrowed("UTF-8"),
            Self::Utf16Le => Cow::Borrowed("UTF-16 LE"),
            Self::Utf16Be => Cow::Borrowed("UTF-16 BE"),
            Self::ShiftJis => Cow::Borrowed("Shift_JIS (CP932)"),
            Self::EucJp => Cow::Borrowed("EUC-JP"),
            Self::Iso2022Jp => Cow::Borrowed("JIS (ISO-2022-JP)"),
            Self::Gbk => Cow::Borrowed("GBK / GB18030"),
            Self::Big5 => Cow::Borrowed("Big5"),
            Self::EucKr => Cow::Borrowed("EUC-KR"),
            Self::Windows1252 => Cow::Borrowed("Latin1 / Windows-1252"),
            Self::Detected(label) => Cow::Owned(label.clone()),
        }
    }

    pub fn detail_text(&self, ui_lang: UiLanguage) -> String {
        build_encoding_detail(ui_lang, "文字コード", "Encoding", self)
    }

    pub fn auto_feedback_text(&self, ui_lang: UiLanguage) -> String {
        build_encoding_detail(ui_lang, "自動判定", "Auto detected", self)
    }

    fn encoding(&self) -> Option<&'static Encoding> {
        match self {
            Self::Utf8 => Encoding::for_label(b"utf-8"),
            Self::Utf16Le => Some(UTF_16LE),
            Self::Utf16Be => Some(UTF_16BE),
            Self::ShiftJis => Some(SHIFT_JIS),
            Self::EucJp => Some(EUC_JP),
            Self::Iso2022Jp => Some(ISO_2022_JP),
            Self::Gbk => Some(GBK),
            Self::Big5 => Some(BIG5),
            Self::EucKr => Some(EUC_KR),
            Self::Windows1252 => Some(WINDOWS_1252),
            Self::Detected(label) => Encoding::for_label(label.as_bytes()),
        }
    }

    fn from_encoding(encoding: &'static Encoding) -> Self {
        match encoding.name() {
            "UTF-8" => Self::Utf8,
            "UTF-16LE" => Self::Utf16Le,
            "UTF-16BE" => Self::Utf16Be,
            "Shift_JIS" => Self::ShiftJis,
            "EUC-JP" => Self::EucJp,
            "ISO-2022-JP" => Self::Iso2022Jp,
            "GBK" => Self::Gbk,
            "Big5" => Self::Big5,
            "EUC-KR" => Self::EucKr,
            "windows-1252" => Self::Windows1252,
            other => Self::Detected(other.to_string()),
        }
    }
}

fn decode_bom_prefixed(bytes: &[u8]) -> io::Result<Option<DecodedText>> {
    if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) || bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00])
    {
        return Err(invalid_data("UTF-32 is not supported"));
    }

    let detected = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        Encoding::for_label(b"utf-8").map(|enc| (enc, FileEncoding::Utf8))
    } else if bytes.starts_with(&[0xFF, 0xFE]) {
        Some((UTF_16LE, FileEncoding::Utf16Le))
    } else if bytes.starts_with(&[0xFE, 0xFF]) {
        Some((UTF_16BE, FileEncoding::Utf16Be))
    } else {
        None
    };
    let Some((encoding, actual)) = detected else {
        return Ok(None);
    };

    let text = decode_with_encoding(bytes, encoding)?;
    Ok(Some(DecodedText {
        text,
        encoding: actual,
    }))
}

fn guess_utf16_without_bom(bytes: &[u8]) -> Option<&'static Encoding> {
    let sample = &bytes[..bytes.len().min(512)];
    let pair_count = sample.len() / 2;
    if pair_count < 4 {
        return None;
    }

    let even_zeros = sample.iter().step_by(2).filter(|&&b| b == 0).count();
    let odd_zeros = sample.iter().skip(1).step_by(2).filter(|&&b| b == 0).count();

    if odd_zeros * 5 >= pair_count * 4 && even_zeros * 5 <= pair_count {
        return Some(UTF_16LE);
    }
    if even_zeros * 5 >= pair_count * 4 && odd_zeros * 5 <= pair_count {
        return Some(UTF_16BE);
    }
    None
}

fn is_probably_binary_text(text: &str) -> bool {
    let mut total = 0usize;
    let mut suspicious = 0usize;

    for ch in text.chars().take(4096) {
        total += 1;
        if ch == '\u{FFFD}' {
            suspicious += 4;
            continue;
        }
        if ch.is_control() && !matches!(ch, '\n' | '\r' | '\t' | '\u{000C}') {
            suspicious += 1;
        }
    }

    total != 0 && suspicious * 20 > total
}

fn build_encoding_detail(
    ui_lang: UiLanguage,
    ja_prefix: &str,
    en_prefix: &str,
    encoding: &FileEncoding,
) -> String {
    let mut parts = vec![match ui_lang {
        UiLanguage::Japanese => format!("{ja_prefix}: {}", encoding.display_label()),
        UiLanguage::English => format!("{en_prefix}: {}", encoding.display_label()),
    }];

    let aliases = encoding_aliases(encoding);
    if !aliases.is_empty() {
        parts.push(match ui_lang {
            UiLanguage::Japanese => format!("別名: {}", aliases.join(", ")),
            UiLanguage::English => format!("Aliases: {}", aliases.join(", ")),
        });
    }

    let near = near_miss_candidates(encoding);
    if !near.is_empty() {
        parts.push(match ui_lang {
            UiLanguage::Japanese => format!("近い候補: {}", near.join(", ")),
            UiLanguage::English => format!("Near matches: {}", near.join(", ")),
        });
    }

    parts.join(" / ")
}

fn encoding_aliases(encoding: &FileEncoding) -> &'static [&'static str] {
    match encoding {
        FileEncoding::Utf8 => &["UTF8", "UTF-8N"],
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => &["UTF-16", "Unicode"],
        FileEncoding::ShiftJis => &["CP932", "Windows-31J", "MS_Kanji"],
        FileEncoding::EucJp => &["EUC"],
        FileEncoding::Iso2022Jp => &["JIS"],
        FileEncoding::Gbk => &["GB18030", "CP936"],
        FileEncoding::Big5 => &["CP950"],
        FileEncoding::EucKr => &["KS X 1001", "CP949"],
        FileEncoding::Windows1252 => &["Latin1", "ISO-8859-1"],
        FileEncoding::Detected(_) => &[],
    }
}

fn near_miss_candidates(encoding: &FileEncoding) -> &'static [&'static str] {
    match encoding {
        FileEncoding::Utf8 => &["UTF-16 LE", "UTF-16 BE"],
        FileEncoding::Utf16Le | FileEncoding::Utf16Be => &["UTF-8", "Shift_JIS (CP932)"],
        FileEncoding::ShiftJis => &["EUC-JP", "JIS (ISO-2022-JP)"],
        FileEncoding::EucJp => &["Shift_JIS (CP932)", "JIS (ISO-2022-JP)"],
        FileEncoding::Iso2022Jp => &["Shift_JIS (CP932)", "EUC-JP"],
        FileEncoding::Gbk => &["Big5", "Latin1 / Windows-1252"],
        FileEncoding::Big5 => &["GBK / GB18030", "Latin1 / Windows-1252"],
        FileEncoding::EucKr => &["Latin1 / Windows-1252", "UTF-8"],
        FileEncoding::Windows1252 => &["UTF-8", "Shift_JIS (CP932)"],
        FileEncoding::Detected(_) => &[],
    }
}

fn invalid_data(msg: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}
