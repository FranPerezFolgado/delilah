//! The Claude model an agent session runs on, chosen by Delilah at each session start.

use serde::{Deserialize, Serialize};

use crate::SpecPhase;

/// A model choice: one of Claude Code's aliases, or `Default` (no `--model`: the account's
/// default model).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentModel {
    Opus,
    Sonnet,
    /// Opus in plan mode, Sonnet otherwise (Claude Code's own alias).
    Opusplan,
    Haiku,
    Default,
}

impl AgentModel {
    pub const ALL: [Self; 5] = [
        Self::Opus,
        Self::Sonnet,
        Self::Opusplan,
        Self::Haiku,
        Self::Default,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Opus => "opus",
            Self::Sonnet => "sonnet",
            Self::Opusplan => "opusplan",
            Self::Haiku => "haiku",
            Self::Default => "default",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == s)
    }

    /// What goes after `--model`; `None` for `Default`.
    pub fn flag(self) -> Option<&'static str> {
        (self != Self::Default).then(|| self.name())
    }
}

/// The model of each phase (the `agent-models` preference).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhaseModels {
    pub specify: AgentModel,
    pub plan: AgentModel,
    pub implement: AgentModel,
    pub review: AgentModel,
}

impl Default for PhaseModels {
    /// Thinking on Opus, typing on Sonnet.
    fn default() -> Self {
        Self {
            specify: AgentModel::Opus,
            plan: AgentModel::Opus,
            implement: AgentModel::Sonnet,
            review: AgentModel::Sonnet,
        }
    }
}

impl PhaseModels {
    pub fn of(&self, phase: SpecPhase) -> AgentModel {
        match phase {
            SpecPhase::Specify => self.specify,
            SpecPhase::Plan => self.plan,
            SpecPhase::Implement => self.implement,
            SpecPhase::Review => self.review,
        }
    }
}

/// The model a new session starts on: the task's own choice, else its phase's.
/// `None`: pass no `--model`.
pub fn resolve(
    task: Option<AgentModel>,
    phase: SpecPhase,
    models: &PhaseModels,
) -> Option<&'static str> {
    task.unwrap_or_else(|| models.of(phase)).flag()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_default_means_no_flag() {
        for m in AgentModel::ALL {
            assert_eq!(AgentModel::parse(m.name()), Some(m));
            let json = serde_json::to_value(m).unwrap();
            assert_eq!(json, m.name());
        }
        assert_eq!(AgentModel::parse("gpt"), None);
        assert_eq!(AgentModel::Default.flag(), None);
        assert_eq!(AgentModel::Opusplan.flag(), Some("opusplan"));
    }

    #[test]
    fn the_task_wins_over_the_phase() {
        let models = PhaseModels::default();
        assert_eq!(resolve(None, SpecPhase::Specify, &models), Some("opus"));
        assert_eq!(resolve(None, SpecPhase::Implement, &models), Some("sonnet"));
        assert_eq!(
            resolve(Some(AgentModel::Haiku), SpecPhase::Specify, &models),
            Some("haiku")
        );
        // A task on "default" passes nothing, whatever its phase says.
        assert_eq!(
            resolve(Some(AgentModel::Default), SpecPhase::Plan, &models),
            None
        );
        let all_default = PhaseModels {
            review: AgentModel::Default,
            ..models
        };
        assert_eq!(resolve(None, SpecPhase::Review, &all_default), None);
    }

    #[test]
    fn phase_models_reject_unknown_names_and_missing_phases() {
        let ok: PhaseModels = serde_json::from_str(
            r#"{"specify":"opus","plan":"opusplan","implement":"sonnet","review":"default"}"#,
        )
        .unwrap();
        assert_eq!(ok.review, AgentModel::Default);
        assert!(
            serde_json::from_str::<PhaseModels>(
                r#"{"specify":"gpt","plan":"opus","implement":"opus","review":"opus"}"#
            )
            .is_err()
        );
        assert!(serde_json::from_str::<PhaseModels>(r#"{"specify":"opus"}"#).is_err());
    }
}
