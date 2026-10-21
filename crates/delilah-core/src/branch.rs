//! Task branch names: `feature/<KEY>-<short>` (Jira) or `feature/<short>` (free).

/// Every task branch starts with this.
pub const PREFIX: &str = "feature/";
/// Longest short part of a branch name.
pub const MAX_SHORT: usize = 50;

/// Words dropped from a title before it becomes a short name.
const FILLER: &[&str] = &[
    "de", "la", "el", "los", "las", "un", "una", "y", "o", "en", "con", "por", "para", "que",
    "del", "al", "a", "the", "of", "to", "for", "and", "or", "in", "on", "with", "an",
];

/// Lowercase ASCII words of `text`: accents transliterated, anything else is a separator.
fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars().flat_map(char::to_lowercase) {
        let mapped = match c {
            'á' | 'à' | 'ä' | 'â' | 'ã' | 'å' => "a",
            'é' | 'è' | 'ë' | 'ê' => "e",
            'í' | 'ì' | 'ï' | 'î' => "i",
            'ó' | 'ò' | 'ö' | 'ô' | 'õ' | 'ø' => "o",
            'ú' | 'ù' | 'ü' | 'û' => "u",
            'ñ' => "n",
            'ç' => "c",
            'ß' => "ss",
            c if c.is_ascii_alphanumeric() => {
                cur.push(c);
                continue;
            }
            _ => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                continue;
            }
        };
        cur.push_str(mapped);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Joins words with `-`, cutting at the last word boundary that fits in `max`.
fn join_max(words: &[String], max: usize) -> String {
    let mut out = String::new();
    for w in words {
        let extra = if out.is_empty() { w.len() } else { w.len() + 1 };
        if out.len() + extra > max {
            if out.is_empty() {
                out = w.chars().take(max).collect();
            }
            break;
        }
        if !out.is_empty() {
            out.push('-');
        }
        out.push_str(w);
    }
    out
}

/// Lowercase ASCII/digits joined by single `-`, at most `max` characters, cut at a word boundary.
pub fn slug(text: &str, max: usize) -> String {
    join_max(&words(text), max)
}

/// Short name from a task title: filler words dropped, at most 6 words.
/// Filler words are kept when nothing else is left. Empty when there is nothing usable.
pub fn title_slug(title: &str) -> String {
    let all = words(title);
    let kept: Vec<String> = all
        .iter()
        .filter(|w| !FILLER.contains(&w.as_str()))
        .take(6)
        .cloned()
        .collect();
    let chosen = if kept.is_empty() {
        all.into_iter().take(6).collect()
    } else {
        kept
    };
    join_max(&chosen, MAX_SHORT)
}

/// Full branch name. `key` is used only for Jira tasks (`!free`). `short` is passed through
/// `slug`, so callers can hand over raw text.
pub fn compose(key: Option<&str>, short: &str, free: bool) -> String {
    let short = slug(short, MAX_SHORT);
    match key {
        Some(k) if !free => format!("{PREFIX}{k}-{short}"),
        _ => format!("{PREFIX}{short}"),
    }
}

/// Branch name from user input: leading `feature/` stripped, the rest slugged.
/// `None` when nothing usable is left.
pub fn custom(input: &str) -> Option<String> {
    let mut rest = input.trim();
    while let Some(r) = rest.strip_prefix(PREFIX) {
        rest = r;
    }
    let s = slug(rest, MAX_SHORT);
    (!s.is_empty()).then(|| format!("{PREFIX}{s}"))
}

/// Branch name when there is no short part: `feature/<KEY>` (Jira) or `feature/<key lowercased>`.
pub fn fallback(key: &str, free: bool) -> String {
    if free {
        format!("{PREFIX}{}", key.to_lowercase())
    } else {
        format!("{PREFIX}{key}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_transliterates_and_drops_symbols() {
        assert_eq!(slug("Añadir réplica ÜÇß", 50), "anadir-replica-ucss");
        assert_eq!(slug("fix 🚀 the *bug*", 50), "fix-the-bug");
    }

    #[test]
    fn slug_collapses_and_trims_dashes() {
        assert_eq!(slug("--a---b__c  d--", 50), "a-b-c-d");
        assert_eq!(slug("!!!", 50), "");
    }

    #[test]
    fn slug_cuts_at_word_boundary() {
        assert_eq!(slug("aaaa bbbb cccc dddd", 12), "aaaa-bbbb");
        let long = "word ".repeat(30);
        let s = slug(&long, 50);
        assert!(s.len() <= 50 && s.ends_with("word") && !s.ends_with('-'));
        assert_eq!(slug("abcdefghij", 5), "abcde");
    }

    #[test]
    fn title_slug_drops_filler_and_limits_words() {
        assert_eq!(
            title_slug("Las tareas deben crearse con nombre de rama"),
            "tareas-deben-crearse-nombre-rama"
        );
        assert_eq!(
            title_slug("one two three four five six seven eight"),
            "one-two-three-four-five-six"
        );
    }

    #[test]
    fn title_slug_keeps_filler_when_alone() {
        assert_eq!(title_slug("de la"), "de-la");
        assert_eq!(title_slug("🚀 !!"), "");
    }

    #[test]
    fn compose_jira_and_free() {
        assert_eq!(
            compose(Some("PROJ-12"), "retry-failed-payments", false),
            "feature/PROJ-12-retry-failed-payments"
        );
        assert_eq!(
            compose(Some("PROJ-12"), "retry-failed-payments", true),
            "feature/retry-failed-payments"
        );
        assert_eq!(
            compose(None, "Retry Payments!", true),
            "feature/retry-payments"
        );
    }

    #[test]
    fn custom_normalises_input() {
        assert_eq!(
            custom("Arreglo del login").as_deref(),
            Some("feature/arreglo-del-login")
        );
        assert_eq!(custom("feature/feature/x").as_deref(), Some("feature/x"));
        assert_eq!(custom("fix/login").as_deref(), Some("feature/fix-login"));
        assert_eq!(custom("///"), None);
    }

    #[test]
    fn fallback_by_kind() {
        assert_eq!(fallback("PROJ-1", false), "feature/PROJ-1");
        assert_eq!(fallback("LIBRE-6", true), "feature/libre-6");
    }
}
