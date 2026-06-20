//! 型ヒント補助ルールの編集ウィンドウ

use std::path::PathBuf;

use eframe::egui;

use crate::app::AstGrepApp;
use crate::type_hint_config::{
    CppBinaryOpRule, CppCallableRule, CppConstantRule, CppFieldRule, CppMethodRule,
    CppTypeHintRules, PendingTypeHintRuleDraft, TypeHintConfigFile, TypeHintRuleKind,
    read_type_hint_config_file, write_type_hint_config_file,
};

#[derive(Debug, Clone, Default)]
pub struct TypeHintConfigEditForm {
    pub enabled: bool,
    pub class: String,
    pub method: String,
    pub name: String,
    pub field: String,
    pub arity: String,
    pub params: String,
    pub returns: String,
    pub ty: String,
    pub op: String,
    pub lhs: String,
    pub rhs: String,
}

impl TypeHintConfigEditForm {
    pub fn from_draft(draft: &PendingTypeHintRuleDraft) -> Self {
        Self {
            enabled: true,
            class: draft.class.clone(),
            method: draft.method.clone(),
            name: draft.name.clone(),
            field: draft.field.clone(),
            arity: draft.arity.map(|n| n.to_string()).unwrap_or_default(),
            params: draft.params.join("\n"),
            returns: draft.returns.clone(),
            ty: draft.ty.clone(),
            op: draft.op.clone(),
            lhs: draft.lhs.clone(),
            rhs: draft.rhs.clone(),
        }
    }

    fn parse_params(&self) -> Vec<String> {
        self.params
            .lines()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .collect()
    }

    fn parse_arity(&self) -> Option<usize> {
        let s = self.arity.trim();
        if s.is_empty() {
            None
        } else {
            s.parse().ok()
        }
    }
}

pub fn show(app: &mut AstGrepApp, ctx: &egui::Context) {
    if !app.show_type_hint_config {
        return;
    }

    let t = app.tr();
    let mut open = app.show_type_hint_config;
    let mut load_path: Option<PathBuf> = None;
    let mut save_path: Option<PathBuf> = None;
    let mut apply_new = false;
    let mut delete_selected = false;
    let mut duplicate_selected = false;
    let mut revert = false;

    egui::Window::new(t.type_hint_config_window_title())
        .open(&mut open)
        .default_size([960.0, 620.0])
        .resizable(true)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                let path = if app.type_hint_config_path.is_empty() {
                    t.type_hint_config_no_file()
                } else {
                    app.type_hint_config_path.as_str()
                };
                ui.label(format!(
                    "{}  |  {}: {}",
                    path,
                    t.type_hint_config_rule_count_label(),
                    app.type_hint_config.rule_count()
                ));
                if app.type_hint_config_dirty {
                    ui.label(
                        egui::RichText::new(t.type_hint_config_unsaved())
                            .color(egui::Color32::from_rgb(220, 180, 90)),
                    );
                }
            });

            if let Some(draft) = &app.pending_type_hint_rule_draft {
                egui::Frame::group(ui.style())
                    .fill(egui::Color32::from_rgb(40, 48, 62))
                    .show(ui, |ui| {
                        ui.label(t.type_hint_config_from_result_banner());
                        ui.monospace(format!(
                            "{}  [{}]  {}",
                            draft.column_key, draft.kind_label, draft.source_snippet
                        ));
                        if !draft.file.is_empty() {
                            ui.small(format!("{}:{}", draft.file, draft.line));
                        }
                    });
            }

            if let Some(msg) = &app.type_hint_config_status {
                ui.colored_label(egui::Color32::from_rgb(140, 200, 140), msg);
            }

            ui.horizontal(|ui| {
                if ui.button(t.type_hint_config_load_yaml()).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("YAML", &["yaml", "yml"])
                        .pick_file()
                    {
                        load_path = Some(path);
                    }
                }
                if ui.button(t.type_hint_config_save_yaml()).clicked() {
                    let mut dlg = rfd::FileDialog::new()
                        .add_filter("YAML", &["yaml", "yml"])
                        .set_file_name("type-hint-config.yaml");
                    if !app.type_hint_config_path.is_empty() {
                        dlg = dlg.set_file_name(&app.type_hint_config_path);
                    }
                    if let Some(path) = dlg.save_file() {
                        save_path = Some(path);
                    }
                }
                if ui.button(t.type_hint_config_add()).clicked() {
                    apply_new = true;
                }
                if ui
                    .add_enabled(
                        app.type_hint_config_selected.is_some(),
                        egui::Button::new(t.type_hint_config_delete()),
                    )
                    .clicked()
                {
                    delete_selected = true;
                }
                if ui
                    .add_enabled(
                        app.type_hint_config_selected.is_some(),
                        egui::Button::new(t.type_hint_config_duplicate()),
                    )
                    .clicked()
                {
                    duplicate_selected = true;
                }
                if ui.button(t.type_hint_config_revert()).clicked() {
                    revert = true;
                }
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(280.0);
                    for kind in [
                        TypeHintRuleKind::Methods,
                        TypeHintRuleKind::Functions,
                        TypeHintRuleKind::Macros,
                        TypeHintRuleKind::Constants,
                        TypeHintRuleKind::Fields,
                        TypeHintRuleKind::BinaryOps,
                    ] {
                        let label = kind_label(t, kind);
                        let selected = app.type_hint_config_kind == kind;
                        if ui.selectable_label(selected, label).clicked() {
                            app.type_hint_config_kind = kind;
                            app.type_hint_config_selected = None;
                            load_form_from_selection(app);
                        }
                    }
                });

                ui.separator();

                ui.vertical(|ui| {
                    ui.set_min_width(520.0);
                    egui::ScrollArea::vertical()
                        .max_height(220.0)
                        .show(ui, |ui| {
                            list_rules(ui, app, t);
                        });

                    ui.separator();
                    show_edit_form(ui, app, t);
                });
            });

            ui.separator();
            ui.label(t.type_hint_config_research_hint());
        });

    app.show_type_hint_config = open;

    if let Some(path) = load_path {
        match read_type_hint_config_file(&path) {
            Ok(file) => {
                app.type_hint_config = file.into();
                app.type_hint_config_path = path.to_string_lossy().to_string();
                app.type_hint_config_dirty = false;
                app.type_hint_config_status = Some(t.type_hint_config_loaded_ok());
            }
            Err(e) => {
                app.type_hint_config_status = Some(t.type_hint_config_error_fmt(&e.to_string()));
            }
        }
    }

    if let Some(path) = save_path {
        let file = TypeHintConfigFile::from_config(&app.type_hint_config);
        match write_type_hint_config_file(&path, &file) {
            Ok(()) => {
                app.type_hint_config_path = path.to_string_lossy().to_string();
                app.type_hint_config_dirty = false;
                app.type_hint_config_status = Some(t.type_hint_config_saved_ok());
            }
            Err(e) => {
                app.type_hint_config_status = Some(t.type_hint_config_error_fmt(&e.to_string()));
            }
        }
    }

    if apply_new {
        if let Some(draft) = app.pending_type_hint_rule_draft.take() {
            app.type_hint_config_kind = draft.kind;
            app.type_hint_config_edit = TypeHintConfigEditForm::from_draft(&draft);
            app.type_hint_config_selected = None;
        } else {
            app.type_hint_config_edit = TypeHintConfigEditForm {
                enabled: true,
                ..Default::default()
            };
            app.type_hint_config_selected = None;
        }
        app.type_hint_config_dirty = true;
    }

    if delete_selected {
        if let Some((kind, idx)) = app.type_hint_config_selected.take() {
            delete_rule(&mut app.type_hint_config.cpp, kind, idx);
            app.type_hint_config_dirty = true;
            app.type_hint_config_edit = TypeHintConfigEditForm::default();
        }
    }

    if duplicate_selected {
        if let Some((kind, idx)) = app.type_hint_config_selected {
            duplicate_rule(&mut app.type_hint_config.cpp, kind, idx);
            app.type_hint_config_dirty = true;
        }
    }

    if revert {
        app.type_hint_config_edit = TypeHintConfigEditForm::default();
        app.type_hint_config_selected = None;
        app.pending_type_hint_rule_draft = None;
    }
}

fn kind_label(t: crate::i18n::Tr, kind: TypeHintRuleKind) -> &'static str {
    match kind {
        TypeHintRuleKind::Methods => t.type_hint_config_tab_methods(),
        TypeHintRuleKind::Functions => t.type_hint_config_tab_functions(),
        TypeHintRuleKind::Macros => t.type_hint_config_tab_macros(),
        TypeHintRuleKind::Constants => t.type_hint_config_tab_constants(),
        TypeHintRuleKind::Fields => t.type_hint_config_tab_fields(),
        TypeHintRuleKind::BinaryOps => t.type_hint_config_tab_binary_ops(),
    }
}

fn list_rules(ui: &mut egui::Ui, app: &mut AstGrepApp, t: crate::i18n::Tr) {
    let kind = app.type_hint_config_kind;
    match kind {
        TypeHintRuleKind::Methods => {
            for (i, r) in app.type_hint_config.cpp.methods.iter().enumerate() {
                let label = format!(
                    "{}{}.{} -> {}",
                    if r.enabled { "" } else { "[off] " },
                    r.class,
                    r.method,
                    r.returns
                );
                if ui
                    .selectable_label(
                        app.type_hint_config_selected == Some((kind, i)),
                        label,
                    )
                    .clicked()
                {
                    app.type_hint_config_selected = Some((kind, i));
                    load_form_from_rule(&mut app.type_hint_config_edit, r);
                }
            }
        }
        TypeHintRuleKind::Functions => {
            for (i, r) in app.type_hint_config.cpp.functions.iter().enumerate() {
                let label = format!(
                    "{}{} -> {}",
                    if r.enabled { "" } else { "[off] " },
                    r.name,
                    r.returns
                );
                if ui
                    .selectable_label(
                        app.type_hint_config_selected == Some((kind, i)),
                        label,
                    )
                    .clicked()
                {
                    app.type_hint_config_selected = Some((kind, i));
                    load_form_from_callable(&mut app.type_hint_config_edit, r);
                }
            }
        }
        TypeHintRuleKind::Macros => {
            for (i, r) in app.type_hint_config.cpp.macros.iter().enumerate() {
                let label = format!(
                    "{}{} -> {}",
                    if r.enabled { "" } else { "[off] " },
                    r.name,
                    r.returns
                );
                if ui
                    .selectable_label(
                        app.type_hint_config_selected == Some((kind, i)),
                        label,
                    )
                    .clicked()
                {
                    app.type_hint_config_selected = Some((kind, i));
                    load_form_from_callable(&mut app.type_hint_config_edit, r);
                }
            }
        }
        TypeHintRuleKind::Constants => {
            for (i, r) in app.type_hint_config.cpp.constants.iter().enumerate() {
                let label = format!(
                    "{}{} : {}",
                    if r.enabled { "" } else { "[off] " },
                    r.name,
                    r.ty
                );
                if ui
                    .selectable_label(
                        app.type_hint_config_selected == Some((kind, i)),
                        label,
                    )
                    .clicked()
                {
                    app.type_hint_config_selected = Some((kind, i));
                    app.type_hint_config_edit = TypeHintConfigEditForm {
                        enabled: r.enabled,
                        name: r.name.clone(),
                        ty: r.ty.clone(),
                        ..Default::default()
                    };
                }
            }
        }
        TypeHintRuleKind::Fields => {
            for (i, r) in app.type_hint_config.cpp.fields.iter().enumerate() {
                let label = format!(
                    "{}{}.{} : {}",
                    if r.enabled { "" } else { "[off] " },
                    r.class,
                    r.field,
                    r.ty
                );
                if ui
                    .selectable_label(
                        app.type_hint_config_selected == Some((kind, i)),
                        label,
                    )
                    .clicked()
                {
                    app.type_hint_config_selected = Some((kind, i));
                    app.type_hint_config_edit = TypeHintConfigEditForm {
                        enabled: r.enabled,
                        class: r.class.clone(),
                        field: r.field.clone(),
                        ty: r.ty.clone(),
                        ..Default::default()
                    };
                }
            }
        }
        TypeHintRuleKind::BinaryOps => {
            for (i, r) in app.type_hint_config.cpp.binary_ops.iter().enumerate() {
                let label = format!(
                    "{}{} {} {} -> {}",
                    if r.enabled { "" } else { "[off] " },
                    r.lhs,
                    r.op,
                    r.rhs,
                    r.returns
                );
                if ui
                    .selectable_label(
                        app.type_hint_config_selected == Some((kind, i)),
                        label,
                    )
                    .clicked()
                {
                    app.type_hint_config_selected = Some((kind, i));
                    app.type_hint_config_edit = TypeHintConfigEditForm {
                        enabled: r.enabled,
                        op: r.op.clone(),
                        lhs: r.lhs.clone(),
                        rhs: r.rhs.clone(),
                        returns: r.returns.clone(),
                        ..Default::default()
                    };
                }
            }
        }
    }
    if rule_count_for_kind(&app.type_hint_config.cpp, kind) == 0 {
        ui.label(t.type_hint_config_empty_list());
    }
}

fn show_edit_form(ui: &mut egui::Ui, app: &mut AstGrepApp, t: crate::i18n::Tr) {
    const FIELD_W: f32 = 400.0;
    const PARAMS_H: f32 = 72.0;

    let form = &mut app.type_hint_config_edit;
    let mut apply = false;

    ui.checkbox(&mut form.enabled, t.type_hint_config_enabled());

    egui::Grid::new("type_hint_config_edit_grid")
        .num_columns(2)
        .spacing([8.0, 6.0])
        .show(ui, |ui| {
            match app.type_hint_config_kind {
                TypeHintRuleKind::Methods => {
                    ui.label("class");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.class),
                    );
                    ui.label("method");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.method),
                    );
                    ui.label("arity");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.arity),
                    );
                    ui.label("params");
                    ui.add_sized(
                        [FIELD_W, PARAMS_H],
                        egui::TextEdit::multiline(&mut form.params).desired_rows(3),
                    )
                    .on_hover_text(t.type_hint_config_params_hint());
                    ui.label("returns");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.returns),
                    );
                }
                TypeHintRuleKind::Functions | TypeHintRuleKind::Macros => {
                    ui.label("name");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.name),
                    );
                    ui.label("arity");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.arity),
                    );
                    ui.label("params");
                    ui.add_sized(
                        [FIELD_W, PARAMS_H],
                        egui::TextEdit::multiline(&mut form.params).desired_rows(3),
                    )
                    .on_hover_text(t.type_hint_config_params_hint());
                    ui.label("returns");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.returns),
                    );
                }
                TypeHintRuleKind::Constants => {
                    ui.label("name");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.name),
                    );
                    ui.label("type");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.ty),
                    );
                }
                TypeHintRuleKind::Fields => {
                    ui.label("class");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.class),
                    );
                    ui.label("field");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.field),
                    );
                    ui.label("type");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.ty),
                    );
                }
                TypeHintRuleKind::BinaryOps => {
                    ui.label("op");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.op),
                    );
                    ui.label("lhs");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.lhs),
                    );
                    ui.label("rhs");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.rhs),
                    );
                    ui.label("returns");
                    ui.add_sized(
                        [FIELD_W, ui.spacing().interact_size.y],
                        egui::TextEdit::singleline(&mut form.returns),
                    );
                }
            }
        });

    if ui.button(t.type_hint_config_apply()).clicked() {
        apply = true;
    }

    if apply {
        if apply_form_to_rules(app) {
            app.type_hint_config_dirty = true;
            app.type_hint_config_status = Some(t.type_hint_config_applied_ok());
            app.pending_type_hint_rule_draft = None;
        } else {
            app.type_hint_config_status = Some(t.type_hint_config_validation_error());
        }
    }
}

fn apply_form_to_rules(app: &mut AstGrepApp) -> bool {
    let form = app.type_hint_config_edit.clone();
    let kind = app.type_hint_config_kind;
    match kind {
        TypeHintRuleKind::Methods => {
            if form.class.trim().is_empty()
                || form.method.trim().is_empty()
                || form.returns.trim().is_empty()
            {
                return false;
            }
            let rule = CppMethodRule {
                class: form.class.trim().to_string(),
                method: form.method.trim().to_string(),
                arity: form.parse_arity(),
                params: form.parse_params(),
                returns: form.returns.trim().to_string(),
                enabled: form.enabled,
            };
            upsert_method(&mut app.type_hint_config.cpp, app.type_hint_config_selected, rule);
        }
        TypeHintRuleKind::Functions | TypeHintRuleKind::Macros => {
            if form.name.trim().is_empty() || form.returns.trim().is_empty() {
                return false;
            }
            let rule = CppCallableRule {
                name: form.name.trim().to_string(),
                arity: form.parse_arity(),
                params: form.parse_params(),
                returns: form.returns.trim().to_string(),
                enabled: form.enabled,
            };
            upsert_callable(
                &mut app.type_hint_config.cpp,
                kind,
                app.type_hint_config_selected,
                rule,
            );
        }
        TypeHintRuleKind::Constants => {
            if form.name.trim().is_empty() || form.ty.trim().is_empty() {
                return false;
            }
            let rule = CppConstantRule {
                name: form.name.trim().to_string(),
                ty: form.ty.trim().to_string(),
                enabled: form.enabled,
            };
            upsert_constant(&mut app.type_hint_config.cpp, app.type_hint_config_selected, rule);
        }
        TypeHintRuleKind::Fields => {
            if form.class.trim().is_empty()
                || form.field.trim().is_empty()
                || form.ty.trim().is_empty()
            {
                return false;
            }
            let rule = CppFieldRule {
                class: form.class.trim().to_string(),
                field: form.field.trim().to_string(),
                ty: form.ty.trim().to_string(),
                enabled: form.enabled,
            };
            upsert_field(&mut app.type_hint_config.cpp, app.type_hint_config_selected, rule);
        }
        TypeHintRuleKind::BinaryOps => {
            if form.op.trim().is_empty()
                || form.lhs.trim().is_empty()
                || form.rhs.trim().is_empty()
                || form.returns.trim().is_empty()
            {
                return false;
            }
            let rule = CppBinaryOpRule {
                op: form.op.trim().to_string(),
                lhs: form.lhs.trim().to_string(),
                rhs: form.rhs.trim().to_string(),
                returns: form.returns.trim().to_string(),
                enabled: form.enabled,
            };
            upsert_binary(&mut app.type_hint_config.cpp, app.type_hint_config_selected, rule);
        }
    }
    true
}

fn load_form_from_selection(app: &mut AstGrepApp) {
    if let Some((kind, idx)) = app.type_hint_config_selected {
        match kind {
            TypeHintRuleKind::Methods => {
                if let Some(r) = app.type_hint_config.cpp.methods.get(idx) {
                    load_form_from_rule(&mut app.type_hint_config_edit, r);
                }
            }
            TypeHintRuleKind::Functions => {
                if let Some(r) = app.type_hint_config.cpp.functions.get(idx) {
                    load_form_from_callable(&mut app.type_hint_config_edit, r);
                }
            }
            TypeHintRuleKind::Macros => {
                if let Some(r) = app.type_hint_config.cpp.macros.get(idx) {
                    load_form_from_callable(&mut app.type_hint_config_edit, r);
                }
            }
            TypeHintRuleKind::Constants => {
                if let Some(r) = app.type_hint_config.cpp.constants.get(idx) {
                    app.type_hint_config_edit = TypeHintConfigEditForm {
                        enabled: r.enabled,
                        name: r.name.clone(),
                        ty: r.ty.clone(),
                        ..Default::default()
                    };
                }
            }
            TypeHintRuleKind::Fields => {
                if let Some(r) = app.type_hint_config.cpp.fields.get(idx) {
                    app.type_hint_config_edit = TypeHintConfigEditForm {
                        enabled: r.enabled,
                        class: r.class.clone(),
                        field: r.field.clone(),
                        ty: r.ty.clone(),
                        ..Default::default()
                    };
                }
            }
            TypeHintRuleKind::BinaryOps => {
                if let Some(r) = app.type_hint_config.cpp.binary_ops.get(idx) {
                    app.type_hint_config_edit = TypeHintConfigEditForm {
                        enabled: r.enabled,
                        op: r.op.clone(),
                        lhs: r.lhs.clone(),
                        rhs: r.rhs.clone(),
                        returns: r.returns.clone(),
                        ..Default::default()
                    };
                }
            }
        }
    }
}

fn load_form_from_rule(form: &mut TypeHintConfigEditForm, r: &CppMethodRule) {
    *form = TypeHintConfigEditForm {
        enabled: r.enabled,
        class: r.class.clone(),
        method: r.method.clone(),
        arity: r.arity.map(|n| n.to_string()).unwrap_or_default(),
        params: r.params.join("\n"),
        returns: r.returns.clone(),
        ..Default::default()
    };
}

fn load_form_from_callable(form: &mut TypeHintConfigEditForm, r: &CppCallableRule) {
    *form = TypeHintConfigEditForm {
        enabled: r.enabled,
        name: r.name.clone(),
        arity: r.arity.map(|n| n.to_string()).unwrap_or_default(),
        params: r.params.join("\n"),
        returns: r.returns.clone(),
        ..Default::default()
    };
}

fn rule_count_for_kind(cpp: &CppTypeHintRules, kind: TypeHintRuleKind) -> usize {
    match kind {
        TypeHintRuleKind::Methods => cpp.methods.len(),
        TypeHintRuleKind::Functions => cpp.functions.len(),
        TypeHintRuleKind::Macros => cpp.macros.len(),
        TypeHintRuleKind::Constants => cpp.constants.len(),
        TypeHintRuleKind::Fields => cpp.fields.len(),
        TypeHintRuleKind::BinaryOps => cpp.binary_ops.len(),
    }
}

fn delete_rule(cpp: &mut CppTypeHintRules, kind: TypeHintRuleKind, idx: usize) {
    match kind {
        TypeHintRuleKind::Methods => {
            if idx < cpp.methods.len() {
                cpp.methods.remove(idx);
            }
        }
        TypeHintRuleKind::Functions => {
            if idx < cpp.functions.len() {
                cpp.functions.remove(idx);
            }
        }
        TypeHintRuleKind::Macros => {
            if idx < cpp.macros.len() {
                cpp.macros.remove(idx);
            }
        }
        TypeHintRuleKind::Constants => {
            if idx < cpp.constants.len() {
                cpp.constants.remove(idx);
            }
        }
        TypeHintRuleKind::Fields => {
            if idx < cpp.fields.len() {
                cpp.fields.remove(idx);
            }
        }
        TypeHintRuleKind::BinaryOps => {
            if idx < cpp.binary_ops.len() {
                cpp.binary_ops.remove(idx);
            }
        }
    }
}

fn duplicate_rule(cpp: &mut CppTypeHintRules, kind: TypeHintRuleKind, idx: usize) {
    match kind {
        TypeHintRuleKind::Methods => {
            if let Some(r) = cpp.methods.get(idx).cloned() {
                cpp.methods.push(r);
            }
        }
        TypeHintRuleKind::Functions => {
            if let Some(r) = cpp.functions.get(idx).cloned() {
                cpp.functions.push(r);
            }
        }
        TypeHintRuleKind::Macros => {
            if let Some(r) = cpp.macros.get(idx).cloned() {
                cpp.macros.push(r);
            }
        }
        TypeHintRuleKind::Constants => {
            if let Some(r) = cpp.constants.get(idx).cloned() {
                cpp.constants.push(r);
            }
        }
        TypeHintRuleKind::Fields => {
            if let Some(r) = cpp.fields.get(idx).cloned() {
                cpp.fields.push(r);
            }
        }
        TypeHintRuleKind::BinaryOps => {
            if let Some(r) = cpp.binary_ops.get(idx).cloned() {
                cpp.binary_ops.push(r);
            }
        }
    }
}

fn upsert_method(
    cpp: &mut CppTypeHintRules,
    selected: Option<(TypeHintRuleKind, usize)>,
    rule: CppMethodRule,
) {
    if let Some((TypeHintRuleKind::Methods, idx)) = selected {
        if idx < cpp.methods.len() {
            cpp.methods[idx] = rule;
            return;
        }
    }
    cpp.methods.push(rule);
}

fn upsert_callable(
    cpp: &mut CppTypeHintRules,
    kind: TypeHintRuleKind,
    selected: Option<(TypeHintRuleKind, usize)>,
    rule: CppCallableRule,
) {
    let list = if kind == TypeHintRuleKind::Functions {
        &mut cpp.functions
    } else {
        &mut cpp.macros
    };
    if let Some((k, idx)) = selected {
        if k == kind && idx < list.len() {
            list[idx] = rule;
            return;
        }
    }
    list.push(rule);
}

fn upsert_constant(
    cpp: &mut CppTypeHintRules,
    selected: Option<(TypeHintRuleKind, usize)>,
    rule: CppConstantRule,
) {
    if let Some((TypeHintRuleKind::Constants, idx)) = selected {
        if idx < cpp.constants.len() {
            cpp.constants[idx] = rule;
            return;
        }
    }
    cpp.constants.push(rule);
}

fn upsert_field(
    cpp: &mut CppTypeHintRules,
    selected: Option<(TypeHintRuleKind, usize)>,
    rule: CppFieldRule,
) {
    if let Some((TypeHintRuleKind::Fields, idx)) = selected {
        if idx < cpp.fields.len() {
            cpp.fields[idx] = rule;
            return;
        }
    }
    cpp.fields.push(rule);
}

fn upsert_binary(
    cpp: &mut CppTypeHintRules,
    selected: Option<(TypeHintRuleKind, usize)>,
    rule: CppBinaryOpRule,
) {
    if let Some((TypeHintRuleKind::BinaryOps, idx)) = selected {
        if idx < cpp.binary_ops.len() {
            cpp.binary_ops[idx] = rule;
            return;
        }
    }
    cpp.binary_ops.push(rule);
}

#[cfg(test)]
mod tests {
    use super::TypeHintConfigEditForm;

    #[test]
    fn parse_params_splits_on_newlines() {
        let form = TypeHintConfigEditForm {
            params: "LPCTSTR\nint\n".into(),
            ..Default::default()
        };
        assert_eq!(form.parse_params(), vec!["LPCTSTR", "int"]);
    }

    #[test]
    fn parse_params_ignores_blank_lines_and_trims() {
        let form = TypeHintConfigEditForm {
            params: "  LPCTSTR \n\n int \n".into(),
            ..Default::default()
        };
        assert_eq!(form.parse_params(), vec!["LPCTSTR", "int"]);
    }
}
