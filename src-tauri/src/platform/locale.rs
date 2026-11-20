//! The language of the system, for texts the backend shows itself while there is no window
//! (the notification about catching up missed runs). The UI finds its own through the webview.

use super::sys;

/// Language code of the system's display language, e.g. "de"; `None` if it cannot be read.
pub fn system_language() -> Option<String> {
    sys::locale::system_language().map(|tag| primary_subtag(&tag))
}

/// "de-DE", "de_DE.UTF-8" → "de".
fn primary_subtag(tag: &str) -> String {
    tag.split(['-', '_', '.', '@']).next().unwrap_or_default().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::primary_subtag;

    #[test]
    fn language_part_of_a_tag() {
        assert_eq!(primary_subtag("de-DE"), "de");
        assert_eq!(primary_subtag("pt_BR.UTF-8"), "pt");
        assert_eq!(primary_subtag("fr"), "fr");
        assert_eq!(primary_subtag("en_US@euro"), "en");
    }
}
