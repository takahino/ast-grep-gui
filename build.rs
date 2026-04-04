fn main() {
    // Windowsビルド時にアイコンを埋め込む
    #[cfg(target_os = "windows")]
    {
        let icon_path = "assets/icon.ico";
        if std::path::Path::new(icon_path).exists() {
            let mut res = winres::WindowsResource::new();
            res.set_icon(icon_path);
            res.compile().expect("Failed to compile Windows resources");
        }
    }
}
