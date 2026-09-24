//! KUE model router: the only way a question reaches a language model.
//!
//! The router does not decide on convenience. For a task it walks the candidate
//! models in order and takes the first one that is (a) permitted by privacy
//! policy for EVERY data kind the task sends, (b) authorized for the asker, and
//! (c) actually available on this Mac. If none qualifies, the question is not
//! sent anywhere.
//!
//! Candidates in this build:
//!   APPLE_ON_DEVICE — FoundationModels, runs on this Mac (Destination::LocalModel)
//!   EXTERNAL        — no external model is configured (Destination::ExternalModel);
//!                     listed so the refusal is explicit and tested, not implicit.

use crate::authz::{Decision as AuthDecision, Operation};
use crate::privacy::{classify, decide, DataKind, Destination};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelId {
    AppleOnDevice,
    External,
}

impl ModelId {
    pub const fn destination(self) -> Destination {
        match self {
            ModelId::AppleOnDevice => Destination::LocalModel,
            ModelId::External => Destination::ExternalModel,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelTask {
    /// A question from the owner, answered with the owner's current context.
    ConversationWithContext,
}

impl ModelTask {
    pub const fn operation(self) -> Operation {
        match self {
            ModelTask::ConversationWithContext => Operation::AskModelWithPersonalContext,
        }
    }

    /// Every kind the task may send. A candidate must be allowed all of them.
    pub const fn sends(self) -> &'static [DataKind] {
        use DataKind::*;
        match self {
            ModelTask::ConversationWithContext => &[
                OwnerMessage, ModelAnswer, IdentityConclusion, ActivityConclusion, PresenceSummary,
                FrontmostApplication, InputIdleTime, SceneLabels, LightLevel, SensorState,
                SystemCondition, Contradiction, EventRecord,
            ],
        }
    }
}

pub const CANDIDATES: [ModelId; 2] = [ModelId::AppleOnDevice, ModelId::External];

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Considered {
    pub model: ModelId,
    pub outcome: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RouteDecision {
    pub chosen: Option<ModelId>,
    pub considered: Vec<Considered>,
}

/// What the shell knows about model availability right now.
#[derive(Debug, Clone, Copy)]
pub struct Availability {
    pub apple_on_device: bool,
    pub external_configured: bool,
}

/// Routes a task. `authorization` is the decision the core already made for
/// `task.operation()`; the router never makes or upgrades that decision.
pub fn route(task: ModelTask, authorization: &AuthDecision, available: Availability) -> RouteDecision {
    let mut considered = Vec::new();
    if *authorization != AuthDecision::Allow {
        for model in CANDIDATES {
            considered.push(Considered { model, outcome: format!("not considered: {:?}", authorization) });
        }
        return RouteDecision { chosen: None, considered };
    }
    for model in CANDIDATES {
        let dest = model.destination();
        if let Some(kind) = task.sends().iter().find(|k| !decide(classify(**k), dest).is_allow()) {
            considered.push(Considered { model, outcome: format!("refused by privacy policy: {} may not go to {}",
                kind.tag(), dest.tag()) });
            continue;
        }
        let up = match model {
            ModelId::AppleOnDevice => available.apple_on_device,
            ModelId::External => available.external_configured,
        };
        if !up {
            considered.push(Considered { model, outcome: "not available".into() });
            continue;
        }
        considered.push(Considered { model, outcome: "chosen".into() });
        return RouteDecision { chosen: Some(model), considered };
    }
    RouteDecision { chosen: None, considered }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_UP: Availability = Availability { apple_on_device: true, external_configured: true };

    #[test]
    fn the_on_device_model_is_chosen_for_conversation() {
        let d = route(ModelTask::ConversationWithContext, &AuthDecision::Allow, ALL_UP);
        assert_eq!(d.chosen, Some(ModelId::AppleOnDevice));
    }

    #[test]
    fn no_personal_context_goes_to_an_external_model_even_if_one_were_configured() {
        let d = route(ModelTask::ConversationWithContext, &AuthDecision::Allow,
            Availability { apple_on_device: false, external_configured: true });
        assert_eq!(d.chosen, None, "falling back to an external model would send your context off this Mac");
        assert!(d.considered[1].outcome.starts_with("refused by privacy policy"), "{:?}", d.considered);
    }

    #[test]
    fn an_unauthorized_asker_reaches_no_model() {
        for auth in [AuthDecision::Deny("x".into()), AuthDecision::NeedsStrongAuth, AuthDecision::NeedsPhysicalConfirmation] {
            assert_eq!(route(ModelTask::ConversationWithContext, &auth, ALL_UP).chosen, None);
        }
    }

    #[test]
    fn raw_measurements_are_not_part_of_any_task() {
        for kind in [DataKind::CameraFrame, DataKind::AudioSample, DataKind::FaceMeasurement,
                     DataKind::BodyJointPositions, DataKind::HandPositions, DataKind::FaceDescriptor,
                     DataKind::TypedText, DataKind::ScreenContent, DataKind::ClipboardContent] {
            assert!(!ModelTask::ConversationWithContext.sends().contains(&kind), "{kind:?}");
        }
    }
}
