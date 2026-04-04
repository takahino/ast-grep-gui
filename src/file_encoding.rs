use std::io;
use std::path::Path;

use encoding_rs::SHIFT_JIS;

use crate::i18n::UiLanguage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileEncodingPreference {
    Auto,
    Utf8,
    ShiftJis,
}

impl Default for FileEncodingPreference {
    fn default() -> Self {
        Self::Auto
    }
}

impl FileEncodingPreference {
    pub fn display_label(self, ui_lang: UiLanguage) -> &'static str {
        match (ui_lang, self) {
            (UiLanguage::Japanese, Self::Auto) => "自動判定",
            (UiLanguage::Japanese, Self::Utf8) => "UTF-8",
            (UiLanguage::Japanese, Self::ShiftJis) => "Shift_JIS (CP932)",
            (UiLanguage::English, Self::Auto) => "Auto detect",
            (UiLanguage::English, Self::Utf8) => "UTF-8",
            (UiLanguage::English, Self::ShiftJis) => "Shift_JIS (CP932)",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileEncoding {
    Utf8,
    ShiftJis,
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
    if bytes.contains(&0) {
        return Err(invalid_data("binary file"));
    }

    match preference {
        FileEncodingPreference::Auto => decode_auto(bytes),
        FileEncodingPreference::Utf8 => decode_utf8(bytes).map(|text| DecodedText {
            text,
            encoding: FileEncoding::Utf8,
        }),
        FileEncodingPreference::ShiftJis => decode_shift_jis(bytes).map(|text| DecodedText {
            text,
            encoding: FileEncoding::ShiftJis,
        }),
    }
}

fn decode_auto(bytes: &[u8]) -> io::Result<DecodedText> {
    if let Ok(text) = decode_utf8(bytes) {
        return Ok(DecodedText {
            text,
            encoding: FileEncoding::Utf8,
        });
    }

    if let Ok(text) = decode_shift_jis(bytes) {
        return Ok(DecodedText {
            text,
            encoding: FileEncoding::ShiftJis,
        });
    }

    Err(invalid_data("unsupported text encoding"))
}

fn decode_bytes_as(bytes: &[u8], encoding: FileEncoding) -> io::Result<String> {
    match encoding {
        FileEncoding::Utf8 => decode_utf8(bytes),
        FileEncoding::ShiftJis => decode_shift_jis(bytes),
    }
}

fn decode_utf8(bytes: &[u8]) -> io::Result<String> {
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    String::from_utf8(bytes.to_vec()).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn decode_shift_jis(bytes: &[u8]) -> io::Result<String> {
    let (decoded, _, had_errors) = SHIFT_JIS.decode(bytes);
    if had_errors {
        return Err(invalid_data("invalid Shift_JIS data"));
    }
    Ok(decoded.into_owned())
}

fn invalid_data(msg: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg)
}
