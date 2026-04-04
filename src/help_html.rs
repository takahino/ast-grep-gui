//! 埋め込み HTML ヘルプをテンポラリに書き出し、OS の既定ブラウザで開く

use crate::i18n::{Tr, UiLanguage};

pub fn open_pattern_help_in_browser(lang: UiLanguage) {
    let t = Tr(lang);
    let (html, name) = match lang {
        UiLanguage::Japanese => (
            include_str!("../assets/help/pattern-help-ja.html"),
            "ast-grep-gui-pattern-help-ja.html",
        ),
        UiLanguage::English => (
            include_str!("../assets/help/pattern-help-en.html"),
            "ast-grep-gui-pattern-help-en.html",
        ),
    };
    let path = std::env::temp_dir().join(name);
    if let Err(e) = std::fs::write(&path, html.as_bytes()) {
        eprintln!("{} {e}", t.help_err_write_temp());
        return;
    }
    if let Err(e) = open::that(&path) {
        eprintln!("{} {e}", t.help_err_open_browser());
    }
}
