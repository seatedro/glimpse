use std::path::Path;
use std::time::Instant;

use console::{style, Color, Style, Term};
use indicatif::{ProgressBar, ProgressStyle};

use glimpse::code::grammar::Registry;

pub struct ProgressContext {
    bar: ProgressBar,
    start: Instant,
    terminal_width: u16,
    total_files: u64,
    total_calls: u64,
}

impl ProgressContext {
    pub fn new() -> Self {
        let term = Term::stdout();
        let terminal_width = term.size().1;

        let bar = ProgressBar::new(0);
        bar.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.cyan} {msg}\n  {bar:40.cyan/blue} {pos}/{len} ({percent}%)")
                .expect("valid template")
                .progress_chars("━━─"),
        );
        bar.enable_steady_tick(std::time::Duration::from_millis(80));

        Self {
            bar,
            start: Instant::now(),
            terminal_width,
            total_files: 0,
            total_calls: 0,
        }
    }

    pub fn set_indexing_total(&mut self, files: u64) {
        self.total_files = files;
        self.bar.set_length(files + self.total_calls);
    }

    pub fn set_lsp_total(&mut self, calls: u64) {
        self.total_calls = calls;
        self.bar.set_length(self.total_files + calls);
    }

    pub fn indexing_file(&self, path: &Path) {
        let ext = path.extension().and_then(|e| e.to_str());
        let colored = self.colorize_path(path, ext);
        let truncated = self.truncate_path(&colored, path);
        self.bar.set_message(format!("Indexing: {}", truncated));
        self.bar.inc(1);
    }

    pub fn resolving_call(&self, source: &Path, target: &str) {
        let ext = source.extension().and_then(|e| e.to_str());
        let colored = self.colorize_path(source, ext);
        let truncated = self.truncate_path(&colored, source);
        self.bar.set_message(format!(
            "Resolving: {} {} {}",
            truncated,
            style("->").dim(),
            target
        ));
        self.bar.inc(1);
    }

    pub fn lsp_warming(&self, server: &str) {
        self.bar.set_message(format!("Warming up {}...", server));
    }

    pub fn scanning(&self) {
        self.bar.set_message("Scanning files...");
    }

    fn colorize_path(&self, path: &Path, ext: Option<&str>) -> String {
        let path_str = path.display().to_string();

        let color = ext
            .and_then(|e| {
                let registry = Registry::global();
                registry
                    .get_by_extension(e)
                    .and_then(|lang| lang.color.as_ref().and_then(|c| hex_to_color(c)))
            })
            .unwrap_or(Color::White);

        Style::new().fg(color).apply_to(&path_str).to_string()
    }

    fn truncate_path(&self, colored: &str, original: &Path) -> String {
        let max_len = (self.terminal_width as usize).saturating_sub(25);
        let plain_len = original.display().to_string().len();

        if plain_len <= max_len {
            return colored.to_string();
        }

        let components: Vec<_> = original.components().collect();
        if components.len() <= 2 {
            return colored.to_string();
        }

        let file_name = original.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let parent = original
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("");

        let ext = original.extension().and_then(|e| e.to_str());
        let truncated_path = format!(".../{}/{}", parent, file_name);
        self.colorize_path(Path::new(&truncated_path), ext)
    }

    pub fn finish(&self, summary: &str) {
        let elapsed = self.start.elapsed();
        self.bar.finish_with_message(format!(
            "{} {} in {:.1}s",
            style("Done").green(),
            summary,
            elapsed.as_secs_f64()
        ));
    }

    pub fn finish_clear(&self) {
        self.bar.finish_and_clear();
    }
}

impl Default for ProgressContext {
    fn default() -> Self {
        Self::new()
    }
}

fn hex_to_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;

    Some(Color::Color256(rgb_to_ansi256(r, g, b)))
}

fn rgb_to_ansi256(r: u8, g: u8, b: u8) -> u8 {
    if r == g && g == b {
        if r < 8 {
            return 16;
        }
        if r > 248 {
            return 231;
        }
        return ((r as f32 - 8.0) / 247.0 * 24.0) as u8 + 232;
    }

    let r_idx = (r as f32 / 255.0 * 5.0).round() as u8;
    let g_idx = (g as f32 / 255.0 * 5.0).round() as u8;
    let b_idx = (b as f32 / 255.0 * 5.0).round() as u8;

    16 + 36 * r_idx + 6 * g_idx + b_idx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_to_color() {
        assert!(hex_to_color("#dea584").is_some());
        assert!(hex_to_color("#3178c6").is_some());
        assert!(hex_to_color("#00ADD8").is_some());
        assert!(hex_to_color("invalid").is_none());
        assert!(hex_to_color("#abc").is_none());
    }

    #[test]
    fn test_rgb_to_ansi256() {
        assert!(rgb_to_ansi256(0, 0, 0) >= 16);
        assert!(rgb_to_ansi256(255, 255, 255) <= 255);
        assert!(rgb_to_ansi256(128, 128, 128) >= 232);
    }
}
