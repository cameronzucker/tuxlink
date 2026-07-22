//! Router / depth selection (Routine CI slice 1a, Task 8): asks the model a
//! single, cheap classification question — "minimal" or "full" — about the
//! operator's raw intent text, and maps its answer onto [`Depth`].
//!
//! `Depth::Minimal` collapses the front phases (Intent/Feasibility/Draft) and
//! runs Author -> Ci -> Present; `Full` runs every phase (see
//! [`super::artifacts::Depth`]'s doc comment). Because [`Depth::Minimal`]
//! trims scaffolding a routine might actually need, this router is
//! deliberately **fail-safe**: any answer it cannot confidently parse as
//! "minimal" resolves to [`Depth::Full`] rather than guessing toward the
//! smaller pipeline. More scaffolding, never less.
//!
//! `select_depth` issues exactly one [`PhaseModel::run_phase`] call with no
//! tools (the classification question needs no tool access), so its cost is
//! a single small prompt — the caller (Task 6's engine) is expected to wrap
//! that call's `PhaseTurn::prompt_tokens` into a [`super::artifacts::PhaseRecord`]
//! alongside the other phases, the same way every other phase call is
//! recorded; this module only performs the classification and does not
//! itself build a `PhaseRecord`.

use super::artifacts::Depth;
use super::model::PhaseModel;

/// Build the router's classification prompt for `intent_text`.
///
/// Kept as its own function (rather than inlined into [`select_depth`]) so
/// a caller — or a future test — can inspect exactly what was sent without
/// re-deriving it, mirroring how `StubModel::prompts_seen` lets tests assert
/// on other phases' prompts.
fn classification_prompt(intent_text: &str) -> String {
    format!(
        "You are the router for a routine-building workflow. Given the \
         operator's stated intent below, decide whether it needs the FULL \
         phase pipeline (feasibility check, drafting review, and CI before \
         presenting) or can skip straight to authoring with only a MINIMAL \
         pipeline. Reply with exactly one word: \"minimal\" or \"full\". Do \
         not explain your answer.\n\nIntent: {intent_text}"
    )
}

/// Ask `model` whether `intent_text` needs the full phase pipeline or can
/// run the minimal one, and return the selected [`Depth`].
///
/// Parses `PhaseTurn::final_text` case-insensitively after trimming
/// surrounding whitespace: a text containing "minimal" (and not "full")
/// selects [`Depth::Minimal`]; a text containing "full" selects
/// [`Depth::Full`]. Any other answer — empty, ambiguous, containing both
/// words, or garbage — resolves to [`Depth::Full`], the fail-safe default.
pub async fn select_depth(intent_text: &str, model: &dyn PhaseModel) -> Depth {
    let prompt = classification_prompt(intent_text);
    let turn = model.run_phase(prompt, &[]).await;
    let answer = turn.final_text.trim().to_lowercase();

    let says_minimal = answer.contains("minimal");
    let says_full = answer.contains("full");

    match (says_minimal, says_full) {
        (true, false) => Depth::Minimal,
        _ => Depth::Full,
    }
}

/// Score a chosen depth against a gold-labeled expected depth. Plain
/// equality, kept as a named function so eval harnesses (and tests) read as
/// "did the router get this depth right" rather than a bare `==`.
pub fn score_depth(chosen: Depth, gold: Depth) -> bool {
    chosen == gold
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::model::{PhaseTurn, StubModel};

    #[tokio::test]
    async fn minimal_answer_selects_minimal_depth() {
        let stub = StubModel::new(vec![PhaseTurn::text("minimal", 5)]);
        let depth = select_depth("check the mailbox", &stub).await;
        assert_eq!(depth, Depth::Minimal);
    }

    #[tokio::test]
    async fn full_answer_with_whitespace_and_case_selects_full_depth() {
        let stub = StubModel::new(vec![PhaseTurn::text("FULL\n", 5)]);
        let depth = select_depth("build a multi-band digipeater failover routine", &stub).await;
        assert_eq!(depth, Depth::Full);
    }

    #[tokio::test]
    async fn unparseable_answer_fails_safe_to_full_depth() {
        let stub = StubModel::new(vec![PhaseTurn::text("i think... maybe?", 5)]);
        let depth = select_depth("do something", &stub).await;
        assert_eq!(depth, Depth::Full);
    }

    #[test]
    fn score_depth_is_equality() {
        assert!(score_depth(Depth::Full, Depth::Full));
        assert!(!score_depth(Depth::Minimal, Depth::Full));
        assert!(score_depth(Depth::Minimal, Depth::Minimal));
    }
}
