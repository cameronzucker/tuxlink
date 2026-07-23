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

/// Parse a router classification answer into a [`Depth`] — the pure,
/// side-effect-free half of [`select_depth`]'s contract, extracted so a
/// caller that already has a [`super::model::PhaseTurn`] in hand (Task 6's
/// engine, see [`select_depth_with_tokens`]'s doc comment) can classify its
/// `final_text` without re-deriving the fail-safe parsing rule.
///
/// Case-insensitive after trimming surrounding whitespace: a text
/// containing "minimal" (and not "full") selects [`Depth::Minimal`]; a text
/// containing "full" selects [`Depth::Full`]. Any other answer — empty,
/// ambiguous, containing both words, or garbage — resolves to
/// [`Depth::Full`], the fail-safe default.
pub fn parse_depth(answer: &str) -> Depth {
    let answer = answer.trim().to_lowercase();

    let says_minimal = answer.contains("minimal");
    let says_full = answer.contains("full");

    match (says_minimal, says_full) {
        (true, false) => Depth::Minimal,
        _ => Depth::Full,
    }
}

/// Ask `model` whether `intent_text` needs the full phase pipeline or can
/// run the minimal one, and return the selected [`Depth`].
///
/// Thin wrapper over [`select_depth_with_tokens`] that drops the token
/// count — kept as its own function (rather than folding callers onto the
/// tuple-returning variant) so existing call sites and this module's own
/// tests do not need to change shape.
pub async fn select_depth(intent_text: &str, model: &dyn PhaseModel) -> Depth {
    select_depth_with_tokens(intent_text, model).await.0
}

/// Same classification as [`select_depth`], but also returns the router
/// turn's `prompt_tokens`.
///
/// **Why this exists (Task 6 engine integration note):** the workflow
/// engine records a [`super::artifacts::PhaseRecord`] for every phase it
/// runs, Router included, and a `PhaseRecord` carries `prompt_tokens`.
/// `select_depth` alone has nowhere to hand that number back — it returns
/// only a [`Depth`]. The alternative considered was having the engine
/// rebuild the router prompt itself via `super::phases::build_prompt(PhaseName::Router,
/// ..)` and call [`parse_depth`] on the result; that path was rejected for
/// two reasons. First, `build_prompt` returns only the rendered prompt, with
/// nowhere to hand back the router turn's `prompt_tokens` the `PhaseRecord`
/// needs — this variant reports the token cost alongside the depth. Second,
/// the router classifies from its own [`classification_prompt`] wording, not
/// the generic per-phase instruction `build_prompt` leads with. (Note:
/// `build_prompt` now renders `intent_text` on every phase — the F1 fix — so
/// routing the router through it would no longer DROP the operator's ask; the
/// two reasons above are why the router keeps its own path regardless.)
pub async fn select_depth_with_tokens(intent_text: &str, model: &dyn PhaseModel) -> (Depth, u64) {
    let prompt = classification_prompt(intent_text);
    let turn = model.run_phase(prompt, &[]).await;
    (parse_depth(&turn.final_text), turn.prompt_tokens)
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
    fn parse_depth_matches_select_depths_own_rules() {
        assert_eq!(parse_depth("minimal"), Depth::Minimal);
        assert_eq!(parse_depth("FULL\n"), Depth::Full);
        assert_eq!(parse_depth("i think... maybe?"), Depth::Full);
        assert_eq!(parse_depth("minimal or full, not sure"), Depth::Full);
    }

    #[tokio::test]
    async fn select_depth_with_tokens_returns_both_the_depth_and_the_turns_prompt_tokens() {
        let stub = StubModel::new(vec![PhaseTurn::text("minimal", 17)]);
        let (depth, tokens) = select_depth_with_tokens("check the mailbox", &stub).await;
        assert_eq!(depth, Depth::Minimal);
        assert_eq!(tokens, 17);
        // Same prompt content as `select_depth` — the intent text must still
        // reach the model, which is the whole reason this variant exists
        // instead of routing through `phases::build_prompt`.
        assert_eq!(
            stub.prompts_seen(),
            vec![classification_prompt("check the mailbox")]
        );
    }

    #[test]
    fn score_depth_is_equality() {
        assert!(score_depth(Depth::Full, Depth::Full));
        assert!(!score_depth(Depth::Minimal, Depth::Full));
        assert!(score_depth(Depth::Minimal, Depth::Minimal));
    }
}
