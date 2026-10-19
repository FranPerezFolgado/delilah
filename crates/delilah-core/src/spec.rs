//! A feature's spec as SpecKit leaves it in `specs/<feature>/`: which phase the work
//! is in, judged by the files present, and the checklist in `tasks.md`.

use serde::{Deserialize, Serialize};

/// Where a feature is in the SpecKit flow. Ordered: each phase comes after the last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SpecPhase {
    /// No `plan.md` yet: the spec is being written or clarified.
    Specify,
    /// `plan.md` (and maybe an empty `tasks.md`): designing the change.
    Plan,
    /// `tasks.md` has items left to do.
    Implement,
    /// Every item in `tasks.md` is checked: time to review the branch.
    Review,
}

impl SpecPhase {
    pub fn name(self) -> &'static str {
        match self {
            Self::Specify => "specify",
            Self::Plan => "plan",
            Self::Implement => "implement",
            Self::Review => "review",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        [Self::Specify, Self::Plan, Self::Implement, Self::Review]
            .into_iter()
            .find(|p| p.name() == name)
    }

    /// The phase given which files the spec folder has and what `tasks.md` holds.
    pub fn of(has_plan: bool, tasks: Option<&[SpecGroup]>) -> Self {
        let items = tasks.into_iter().flatten().flat_map(|g| &g.items);
        let (mut total, mut done) = (0, 0);
        for item in items {
            total += 1;
            done += usize::from(item.done);
        }
        match (total, has_plan) {
            (0, false) if tasks.is_none() => Self::Specify,
            (0, _) => Self::Plan,
            _ if done == total => Self::Review,
            _ => Self::Implement,
        }
    }
}

/// One checkbox of `tasks.md`, e.g. `- [x] T012 [P] [US1] Add the retry policy`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecItem {
    /// SpecKit's id, `T012`.
    pub id: Option<String>,
    pub text: String,
    pub done: bool,
    /// `[P]`: can run in parallel with its neighbours.
    pub parallel: bool,
    /// `[US1]`: the user story it belongs to.
    pub story: Option<String>,
}

/// The items under one heading, e.g. "Phase 3: User Story 1".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecGroup {
    pub title: String,
    pub items: Vec<SpecItem>,
}

/// Reads the checklist of a `tasks.md`. Headings start groups; checkboxes before the
/// first heading go in an untitled one. Anything else (notes, dependencies) is skipped.
pub fn parse_tasks(markdown: &str) -> Vec<SpecGroup> {
    let mut groups: Vec<SpecGroup> = Vec::new();
    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if let Some(title) = heading(trimmed) {
            groups.push(SpecGroup {
                title: title.to_string(),
                items: Vec::new(),
            });
        } else if let Some(item) = checkbox(trimmed) {
            if groups.is_empty() {
                groups.push(SpecGroup {
                    title: String::new(),
                    items: Vec::new(),
                });
            }
            groups.last_mut().unwrap().items.push(item);
        }
    }
    groups.retain(|g| !g.items.is_empty());
    groups
}

fn heading(line: &str) -> Option<&str> {
    // The document title (`# Tasks: …`) is not a group.
    let level = line.chars().take_while(|&c| c == '#').count();
    if level < 2 || !line[level..].starts_with(' ') {
        return None;
    }
    Some(line[level..].trim())
}

fn checkbox(line: &str) -> Option<SpecItem> {
    let rest = line
        .strip_prefix("- [")
        .or_else(|| line.strip_prefix("* ["))?;
    let mut chars = rest.chars();
    let mark = chars.next()?;
    let rest = chars.as_str().strip_prefix(']')?.trim_start();
    let done = match mark {
        'x' | 'X' => true,
        ' ' => false,
        _ => return None,
    };
    let mut words = rest.split_whitespace().peekable();
    let mut id = None;
    let mut parallel = false;
    let mut story = None;
    if let Some(w) = words.peek()
        && is_task_id(w)
    {
        id = Some(w.to_string());
        words.next();
    }
    while let Some(w) = words.peek() {
        match *w {
            "[P]" => parallel = true,
            tag if tag.starts_with("[US") && tag.ends_with(']') => {
                story = Some(tag[1..tag.len() - 1].to_string());
            }
            _ => break,
        }
        words.next();
    }
    Some(SpecItem {
        id,
        text: words.collect::<Vec<_>>().join(" "),
        done,
        parallel,
        story,
    })
}

fn is_task_id(word: &str) -> bool {
    let digits = word.strip_prefix('T').unwrap_or("");
    !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TASKS: &str = "# Tasks: Card retries

**Input**: design documents from `/specs/PROJ-1242-card-retries/`

## Phase 1: Setup
- [x] T001 Create the retry module
- [X] T002 [P] Add the config flag

## Phase 3: User Story 1 - Retry declined cards (Priority: P1) 🎯 MVP
- [ ] T010 [P] [US1] Contract test for POST /payments/retry
- [ ] T011 [US1] Implement the retry policy in svc-payments
  - nested notes are not tasks

## Dependencies
Setup before everything.
";

    #[test]
    fn reads_groups_items_and_their_tags() {
        let groups = parse_tasks(TASKS);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].title, "Phase 1: Setup");
        assert_eq!(groups[0].items.len(), 2);
        assert!(groups[0].items.iter().all(|i| i.done));
        let t10 = &groups[1].items[0];
        assert_eq!(t10.id.as_deref(), Some("T010"));
        assert!(t10.parallel && !t10.done);
        assert_eq!(t10.story.as_deref(), Some("US1"));
        assert_eq!(t10.text, "Contract test for POST /payments/retry");
        assert_eq!(
            groups[1].items[1].text,
            "Implement the retry policy in svc-payments"
        );
    }

    #[test]
    fn untitled_checklists_still_count() {
        let groups = parse_tasks("- [ ] do this\n- [x] did that\n");
        assert_eq!(groups[0].title, "");
        assert_eq!(groups[0].items.len(), 2);
        assert_eq!(groups[0].items[0].id, None);
    }

    #[test]
    fn the_phase_follows_the_files_and_the_checklist() {
        assert_eq!(SpecPhase::of(false, None), SpecPhase::Specify);
        assert_eq!(SpecPhase::of(true, None), SpecPhase::Plan);
        assert_eq!(SpecPhase::of(true, Some(&[])), SpecPhase::Plan);
        let groups = parse_tasks(TASKS);
        assert_eq!(SpecPhase::of(true, Some(&groups)), SpecPhase::Implement);
        let all_done = parse_tasks(&TASKS.replace("- [ ]", "- [x]"));
        assert_eq!(SpecPhase::of(true, Some(&all_done)), SpecPhase::Review);
    }
}
