use eframe::egui;

use crate::app::AstGrepApp;
use crate::terminal::{LineKind, TerminalState};

/// ターミナルパネルを描画する
pub fn show(app: &mut AstGrepApp, ui: &mut egui::Ui) {
    let terminal = match app.terminal.as_mut() {
        Some(t) => t,
        None => return,
    };

    let available = ui.available_size();

    // 入力テキストの行数に応じて入力エリアの高さを動的に計算する。
    // これにより入力欄が増えるときは出力エリアが縮み、入力欄が上方向に広がる。
    let line_count = terminal.input.lines().count().max(1) as f32;
    let line_height = 16.0; // モノスペースフォントの概算行高
    let input_area_reserved = (line_count * line_height + 28.0).max(48.0);
    let output_height = (available.y - input_area_reserved - 8.0).max(40.0);

    let scroll_to_bottom = terminal.scroll_to_bottom;
    if scroll_to_bottom {
        terminal.scroll_to_bottom = false;
    }

    // ---- 出力エリア ----
    egui::ScrollArea::vertical()
        .id_salt("terminal_output")
        .max_height(output_height)
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if scroll_to_bottom {
                ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
            }

            let lines_snapshot = terminal
                .lines
                .lock()
                .map(|l| l.clone())
                .unwrap_or_default();

            for line in &lines_snapshot {
                let color = match line.kind {
                    LineKind::Prompt => egui::Color32::from_rgb(100, 160, 255),
                    LineKind::Stdout => egui::Color32::LIGHT_GRAY,
                    LineKind::Stderr => egui::Color32::from_rgb(255, 100, 80),
                };
                ui.label(
                    egui::RichText::new(&line.text)
                        .monospace()
                        .color(color)
                        .size(13.0),
                );
            }
        });

    ui.add_space(4.0);
    ui.separator();

    // ---- 入力行 ----
    ui.horizontal(|ui| {
        // プロンプトラベル
        let prompt = terminal.prompt_str();
        ui.label(
            egui::RichText::new(prompt)
                .monospace()
                .color(egui::Color32::from_rgb(100, 160, 255))
                .size(13.0),
        );

        let input_id = egui::Id::new("terminal_input_field");

        // TextEdit が Enter を処理する前にキーをインターセプト
        let has_focus = ui.ctx().memory(|m| m.has_focus(input_id));
        let (enter_pressed, up_pressed, down_pressed) = if has_focus {
            ui.input_mut(|i| {
                // consume_key は matches_logically を使うため Shift の有無を無視してしまう。
                // そのため手動でイベントを走査し、Shift なし Enter のみを消費する。
                let enter_pos = i.events.iter().position(|e| {
                    matches!(e, egui::Event::Key {
                        key: egui::Key::Enter,
                        pressed: true,
                        modifiers,
                        ..
                    } if !modifiers.shift)
                });
                let enter = if let Some(pos) = enter_pos {
                    i.events.remove(pos);
                    true
                } else {
                    false
                };
                // ↑↓ → 履歴ナビゲート。TextEdit のカーソル移動より優先。
                let up = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
                let down = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                (enter, up, down)
            })
        } else {
            (false, false, false)
        };

        let input_width = ui.available_width() - 56.0;
        // multiline にすることで Shift+Enter が改行として機能する
        ui.add(
            egui::TextEdit::multiline(&mut terminal.input)
                .id(input_id)
                .desired_rows(1)
                .desired_width(input_width)
                .font(egui::TextStyle::Monospace)
                .hint_text("コマンドを入力… (Enter: 実行 / Shift+Enter: 改行)"),
        );

        // 履歴ナビゲーション
        let history_len = terminal.history.len();
        if up_pressed && history_len > 0 {
            let next = match terminal.history_idx {
                None => 0,
                Some(i) => (i + 1).min(history_len - 1),
            };
            terminal.history_idx = Some(next);
            terminal.input = terminal.history[next].clone();
        }
        if down_pressed {
            match terminal.history_idx {
                None => {}
                Some(0) => {
                    terminal.history_idx = None;
                    terminal.input.clear();
                }
                Some(i) => {
                    terminal.history_idx = Some(i - 1);
                    terminal.input = terminal.history[i - 1].clone();
                }
            }
        }

        // コマンド実行
        if enter_pressed {
            execute_input(terminal, ui.ctx().clone());
        }

        // Run ボタン
        if ui.button("Run").clicked() {
            execute_input(terminal, ui.ctx().clone());
        }
    });
}

fn execute_input(terminal: &mut TerminalState, ctx: egui::Context) {
    let cmd = terminal.input.trim().to_string();
    if cmd.is_empty() {
        return;
    }
    terminal.input.clear();
    terminal.run_command(&cmd, ctx);
}
