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
            // Combining marks of decomposed accents belong to the letter before them.
            '\u{300}'..='\u{36f}' => continue,
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

/// A Jira key made safe for a branch name: case kept, only ASCII letters/digits and single
/// `-`, trimmed. `None` when nothing is left.
fn clean_key(key: &str) -> Option<String> {
    let mut out = String::new();
    for c in key.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    (!out.is_empty()).then_some(out)
}

/// Full branch name. `key` is used only for Jira tasks (`!free`). `short` is passed through
/// `slug`, so callers can hand over raw text. `None` when there is neither a usable key nor
/// a usable short part (a bare `feature/` is not a branch).
pub fn compose(key: Option<&str>, short: &str, free: bool) -> Option<String> {
    let short = slug(short, MAX_SHORT);
    let key = key.filter(|_| !free).and_then(clean_key);
    match (key, short.is_empty()) {
        (Some(k), true) => Some(format!("{PREFIX}{k}")),
        (Some(k), false) => Some(format!("{PREFIX}{k}-{short}")),
        (None, true) => None,
        (None, false) => Some(format!("{PREFIX}{short}")),
    }
}

/// Branch name from user input: leading `feature/` stripped, the rest slugged.
/// `None` when nothing usable is left.
pub fn custom(input: &str) -> Option<String> {
    custom_for(input, None, true)
}

/// Like `custom`, for a task with `key`. A Jira task (`!free`) whose input already starts
/// with its key (any case, e.g. a proposal shown to the user) keeps the key in its real case
/// and slugs only the rest; any other input is slugged whole, with no key added.
pub fn custom_for(input: &str, key: Option<&str>, free: bool) -> Option<String> {
    let mut rest = input.trim();
    while let Some(r) = rest.strip_prefix(PREFIX) {
        rest = r;
    }
    if let Some(key) = key.filter(|_| !free).and_then(clean_key)
        && let Some(head) = rest.get(..key.len())
        && head.eq_ignore_ascii_case(&key)
    {
        let tail = &rest[key.len()..];
        if tail
            .chars()
            .next()
            .is_none_or(|c| c == '-' || c == '/' || c.is_whitespace())
        {
            return compose(Some(&key), tail, false);
        }
    }
    let s = slug(rest, MAX_SHORT);
    (!s.is_empty()).then(|| format!("{PREFIX}{s}"))
}

/// Branch name when there is no short part: `feature/<KEY>` (Jira) or `feature/<key lowercased>`.
/// `None` when the key has nothing usable.
pub fn fallback(key: &str, free: bool) -> Option<String> {
    let key = clean_key(key)?;
    Some(if free {
        format!("{PREFIX}{}", key.to_lowercase())
    } else {
        format!("{PREFIX}{key}")
    })
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
            compose(Some("PROJ-12"), "retry-failed-payments", false).as_deref(),
            Some("feature/PROJ-12-retry-failed-payments")
        );
        assert_eq!(
            compose(Some("PROJ-12"), "retry-failed-payments", true).as_deref(),
            Some("feature/retry-failed-payments")
        );
        assert_eq!(
            compose(None, "Retry Payments!", true).as_deref(),
            Some("feature/retry-payments")
        );
    }

    #[test]
    fn compose_sanitises_the_key() {
        assert_eq!(
            compose(Some("PROJ 12"), "x", false).as_deref(),
            Some("feature/PROJ-12-x")
        );
        assert_eq!(
            compose(Some("..~"), "x", false).as_deref(),
            Some("feature/x")
        );
        assert_eq!(compose(Some("..~"), "", false), None);
        assert_eq!(fallback("..~", false), None);
    }

    #[test]
    fn compose_without_a_short_part() {
        assert_eq!(
            compose(Some("PROJ-12"), "", false).as_deref(),
            Some("feature/PROJ-12")
        );
        assert_eq!(compose(Some("PROJ-12"), "!!", true), None);
        assert_eq!(compose(None, "", true), None);
    }

    #[test]
    fn slug_skips_combining_marks() {
        assert_eq!(slug("re\u{301}plica", 50), "replica");
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
    fn custom_for_keeps_the_key_of_a_proposal() {
        let c = |i, k, f| custom_for(i, k, f);
        assert_eq!(
            c("feature/PROJ-12-retry-payments", Some("PROJ-12"), false).as_deref(),
            Some("feature/PROJ-12-retry-payments")
        );
        assert_eq!(
            c("feature/proj-12 Retry", Some("PROJ-12"), false).as_deref(),
            Some("feature/PROJ-12-retry")
        );
        assert_eq!(
            c("Arreglo del login", Some("PROJ-12"), false).as_deref(),
            Some("feature/arreglo-del-login")
        );
        assert_eq!(
            c("feature/feature/x", None, true).as_deref(),
            Some("feature/x")
        );
        assert_eq!(
            c("PROJ-12", Some("PROJ-12"), false).as_deref(),
            Some("feature/PROJ-12")
        );
        assert_eq!(
            c("PROJ-123-x", Some("PROJ-12"), false).as_deref(),
            Some("feature/proj-123-x")
        );
        assert_eq!(
            c("proj-12-x", Some("PROJ-12"), true).as_deref(),
            Some("feature/proj-12-x")
        );
        assert_eq!(c("///", Some("PROJ-12"), false), None);
        assert_eq!(c("///", None, true), None);
    }

    #[test]
    fn fallback_by_kind() {
        assert_eq!(fallback("PROJ-1", false).as_deref(), Some("feature/PROJ-1"));
        assert_eq!(
            fallback("LIBRE-6", true).as_deref(),
            Some("feature/libre-6")
        );
    }
}
