use std::collections::HashMap;

use egui::text::LayoutJob;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::lang::SupportedLanguage;
use crate::search::MatchItem;

pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    /// ファイルパスをキーにしたハイライトキャッシュ
    cache: HashMap<String, Vec<Vec<(Style, String)>>>,
}

impl Highlighter {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            cache: HashMap::new(),
        }
    }

    /// ソースコードをハイライト処理し、行ごとの(Style, text)リストを返す
    pub fn highlight_source(
        &mut self,
        cache_key: &str,
        source: &str,
        lang: SupportedLanguage,
    ) -> &Vec<Vec<(Style, String)>> {
        if !self.cache.contains_key(cache_key) {
            let syntax = self
                .syntax_set
                .find_syntax_by_name(lang.syntect_name())
                .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

            let theme = &self.theme_set.themes["base16-ocean.dark"];
            let mut h = HighlightLines::new(syntax, theme);

            let highlighted: Vec<Vec<(Style, String)>> = LinesWithEndings::from(source)
                .map(|line| {
                    h.highlight_line(line, &self.syntax_set)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|(style, text)| (style, text.to_string()))
                        .collect()
                })
                .collect();

            self.cache.insert(cache_key.to_string(), highlighted);
        }

        &self.cache[cache_key]
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

/// syntect の Color を egui の Color32 に変換
fn syntect_to_egui_color(color: syntect::highlighting::Color) -> egui::Color32 {
    egui::Color32::from_rgba_premultiplied(color.r, color.g, color.b, color.a)
}

/// 行全体の薄い背景色（マッチ行の背景）
const LINE_BG: egui::Color32 = egui::Color32::from_rgba_premultiplied(80, 80, 0, 40);
/// マッチテキスト部分の強い背景色
const MATCH_BG: egui::Color32 = egui::Color32::from_rgba_premultiplied(200, 160, 0, 180);
/// 表示時のタブ幅
const TAB_WIDTH: usize = 4;

fn expand_tabs(text: &str, visual_col: &mut usize) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch == '\t' {
            let spaces = TAB_WIDTH - (*visual_col % TAB_WIDTH);
            for _ in 0..spaces {
                out.push(' ');
            }
            *visual_col += spaces;
        } else {
            out.push(ch);
            *visual_col += 1;
        }
    }
    out
}

fn append_text_with_format(
    job: &mut LayoutJob,
    text: &str,
    format: egui::TextFormat,
    visual_col: &mut usize,
) {
    let expanded = expand_tabs(text, visual_col);
    if expanded.is_empty() {
        return;
    }
    job.append(&expanded, 0.0, format);
}

/// 1行分のハイライトデータを egui LayoutJob のセクションとして追加する
///
/// - `col_highlight`: この行内でテキスト強調する byteオフセット範囲
///   - `None`  → 通常行
///   - `Some(0..usize::MAX)` → 行全体を薄くハイライト（マッチ行だが列不明）
///   - `Some(start..end)` → 列レベルで強調
fn append_highlighted_line(
    job: &mut LayoutJob,
    line_tokens: &[(Style, String)],
    col_highlight: Option<std::ops::Range<usize>>,
    line_number: usize,
    font_size: f32,
) {
    let font_id = egui::FontId::monospace(font_size);
    let is_match_line = col_highlight.is_some();

    // 行番号
    {
        let line_num_text = format!("{:4}│ ", line_number);
        let bg = if is_match_line { LINE_BG } else { egui::Color32::TRANSPARENT };
        job.append(
            &line_num_text,
            0.0,
            egui::TextFormat {
                font_id: font_id.clone(),
                color: egui::Color32::from_gray(110),
                background: bg,
                ..Default::default()
            },
        );
    }

    let mut byte_pos = 0usize;
    let mut visual_col = 0usize;

    for (style, text) in line_tokens {
        // 改行文字を除去
        let text = text.trim_end_matches('\n').trim_end_matches('\r');
        if text.is_empty() {
            continue;
        }

        let token_start = byte_pos;
        let token_end = byte_pos + text.len();
        let fg = syntect_to_egui_color(style.foreground);

        match &col_highlight {
            None => {
                // 通常行
                append_text_with_format(
                    job,
                    text,
                    egui::TextFormat {
                        font_id: font_id.clone(),
                        color: fg,
                        ..Default::default()
                    },
                    &mut visual_col,
                );
            }
            Some(range) => {
                // マッチ行：トークン内で範囲が重なる部分だけ強調
                append_token_with_highlight(
                    job,
                    text,
                    token_start,
                    token_end,
                    range.clone(),
                    fg,
                    &font_id,
                    &mut visual_col,
                );
            }
        }

        byte_pos = token_end;
    }

    // 改行
    job.append(
        "\n",
        0.0,
        egui::TextFormat {
            font_id: font_id.clone(),
            color: egui::Color32::TRANSPARENT,
            ..Default::default()
        },
    );
}

/// トークン文字列を [before] [highlighted] [after] に分割して追記する
fn append_token_with_highlight(
    job: &mut LayoutJob,
    text: &str,
    token_start: usize,
    token_end: usize,
    range: std::ops::Range<usize>,
    fg: egui::Color32,
    font_id: &egui::FontId,
    visual_col: &mut usize,
) {
    // トークンと強調範囲の交差
    let hl_start = range.start.max(token_start);
    let hl_end = range.end.min(token_end);

    if hl_start >= hl_end {
        // 強調なし（ただしマッチ行なので薄い背景）
        append_text_with_format(
            job,
            text,
            egui::TextFormat {
                font_id: font_id.clone(),
                color: fg,
                background: LINE_BG,
                ..Default::default()
            },
            visual_col,
        );
        return;
    }

    // 強調前
    let before_len = hl_start - token_start;
    if before_len > 0 {
        if let Some(before) = text.get(..before_len) {
            append_text_with_format(
                job,
                before,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: fg,
                    background: LINE_BG,
                    ..Default::default()
                },
                visual_col,
            );
        }
    }

    // 強調部分
    let hl_local_start = hl_start - token_start;
    let hl_local_end = hl_end - token_start;
    if let Some(hl_text) = text.get(hl_local_start..hl_local_end) {
        append_text_with_format(
            job,
            hl_text,
            egui::TextFormat {
                font_id: font_id.clone(),
                color: egui::Color32::BLACK,
                background: MATCH_BG,
                ..Default::default()
            },
            visual_col,
        );
    }

    // 強調後
    if let Some(after) = text.get(hl_local_end..) {
        if !after.is_empty() {
            append_text_with_format(
                job,
                after,
                egui::TextFormat {
                    font_id: font_id.clone(),
                    color: fg,
                    background: LINE_BG,
                    ..Default::default()
                },
                visual_col,
            );
        }
    }
}

/// ソース全体の LayoutJob を生成する
///
/// `matches` の行・列情報をもとに、マッチ行を薄くハイライトし
/// マッチテキストの該当列部分を強調表示する。
pub fn build_layout_job(
    highlighted: &[Vec<(Style, String)>],
    matches: &[MatchItem],
    font_size: f32,
) -> LayoutJob {
    build_layout_job_from_line(highlighted, matches, font_size, 1)
}

/// 行番号の開始値を指定して LayoutJob を生成する。
pub fn build_layout_job_from_line(
    highlighted: &[Vec<(Style, String)>],
    matches: &[MatchItem],
    font_size: f32,
    start_line_number: usize,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;

    for (idx, line_tokens) in highlighted.iter().enumerate() {
        let line_number = start_line_number + idx;

        // この行に対するハイライト範囲を決定
        let col_highlight = col_highlight_for_line(line_number, matches);

        append_highlighted_line(&mut job, line_tokens, col_highlight, line_number, font_size);
    }

    job
}

/// 指定行（1-based）に対してハイライトする列範囲を返す
///
/// - 該当するマッチがなければ `None`
/// - 単一行マッチ: `Some(col_start..col_end)`
/// - 複数行マッチの先頭行: `Some(col_start..MAX)` （行末まで）
/// - 複数行マッチの中間行: `Some(0..MAX)` （全体）
/// - 複数行マッチの末尾行: `Some(0..col_end)`
fn col_highlight_for_line(line_number: usize, matches: &[MatchItem]) -> Option<std::ops::Range<usize>> {
    for m in matches {
        if line_number < m.line_start || line_number > m.line_end {
            continue;
        }
        let range = if m.line_start == m.line_end {
            // 単一行マッチ
            m.col_start..m.col_end
        } else if line_number == m.line_start {
            // 先頭行：マッチ開始から行末
            m.col_start..usize::MAX
        } else if line_number == m.line_end {
            // 末尾行：行頭からマッチ終端
            0..m.col_end
        } else {
            // 中間行：全体
            0..usize::MAX
        };
        return Some(range);
    }
    None
}

/// 置換プレビュー用: 置換前・置換後を並べて表示する `LayoutJob`（行単位で差分を色分け）
///
/// - 変更行: 置換前は赤系背景、置換後は緑系背景
/// - 同一行: 通常の前景色、背景なし
pub fn build_rewrite_compare_layout_jobs(
    old_source: &str,
    new_source: &str,
    font_size: f32,
) -> (LayoutJob, LayoutJob) {
    let old_lines: Vec<&str> = old_source.lines().collect();
    let new_lines: Vec<&str> = new_source.lines().collect();
    let max_lines = old_lines.len().max(new_lines.len());
    let mut old_job = LayoutJob::default();
    let mut new_job = LayoutJob::default();
    old_job.wrap.max_width = f32::INFINITY;
    new_job.wrap.max_width = f32::INFINITY;

    let font_id = egui::FontId::monospace(font_size);
    let gutter_w = max_lines.to_string().len().max(3);

    // 置換前: 削除・変更に近いトーン / 置換後: 追加・変更に近いトーン（ダーク UI 向け）
    let bg_before_changed = egui::Color32::from_rgba_premultiplied(130, 55, 55, 110);
    let bg_after_changed = egui::Color32::from_rgba_premultiplied(50, 115, 65, 110);
    let fg_normal = egui::Color32::from_rgba_premultiplied(215, 215, 215, 255);
    let fg_changed = egui::Color32::from_rgba_premultiplied(255, 250, 235, 255);
    let fg_gutter = egui::Color32::from_gray(130);

    for i in 0..max_lines {
        let ol = old_lines.get(i).copied();
        let nl = new_lines.get(i).copied();
        let changed = match (ol, nl) {
            (Some(a), Some(b)) => a != b,
            (Some(_), None) | (None, Some(_)) => true,
            (None, None) => false,
        };

        let gutter = format!("{:>width$} │ ", i + 1, width = gutter_w);
        let gutter_fmt = egui::TextFormat {
            font_id: font_id.clone(),
            color: fg_gutter,
            ..Default::default()
        };
        old_job.append(&gutter, 0.0, gutter_fmt.clone());
        new_job.append(&gutter, 0.0, gutter_fmt);

        let fg = if changed { fg_changed } else { fg_normal };
        let (bg_old, bg_new) = if changed {
            (bg_before_changed, bg_after_changed)
        } else {
            (egui::Color32::TRANSPARENT, egui::Color32::TRANSPARENT)
        };

        let old_fmt = egui::TextFormat {
            font_id: font_id.clone(),
            color: fg,
            background: bg_old,
            ..Default::default()
        };
        let new_fmt = egui::TextFormat {
            font_id: font_id.clone(),
            color: fg,
            background: bg_new,
            ..Default::default()
        };

        let old_text = ol.unwrap_or("");
        let new_text = nl.unwrap_or("");
        old_job.append(old_text, 0.0, old_fmt);
        new_job.append(new_text, 0.0, new_fmt);

        let nl_fmt = egui::TextFormat {
            font_id: font_id.clone(),
            color: fg,
            background: bg_old,
            ..Default::default()
        };
        let nl_fmt2 = egui::TextFormat {
            font_id: font_id.clone(),
            color: fg,
            background: bg_new,
            ..Default::default()
        };
        old_job.append("\n", 0.0, nl_fmt);
        new_job.append("\n", 0.0, nl_fmt2);
    }

    (old_job, new_job)
}
