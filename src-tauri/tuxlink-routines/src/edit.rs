//! Pure fragment-edit operations over a [`RoutineDef`] — the engine half of
//! the edit-verb authoring surface (tuxlink-aqy63, spec
//! `docs/design/routines-edit-verb-authoring.md`).
//!
//! This module mirrors the designer's `defDraft.ts` op-for-op (D8: two skins
//! over one contract): insert-after and insert-into-branch-arm placement,
//! shallow step patching, and the load-bearing branch-arm scrub on removal.
//! Where it deliberately DIFFERS from the designer: an unknown track/step id
//! is an [`EditError`], not a silent no-op — the designer's UI cannot produce
//! a dangling reference, an agent's tool call can, and a silent no-op there
//! is exactly the "reported success, changed nothing" failure the verbs exist
//! to end (D6 outcome 2: precondition failure, no mutation).
//!
//! Every op is pure `&RoutineDef -> RoutineDef`: no store, no validation, no
//! consent handling — the command layer owns the enabled-guard, revision
//! check, validator call, and persistence ordering.

use serde_json::Value;

use crate::types::{Control, InputDecl, OnInterrupted, RoutineDef, Step, StepId, Track, TransmitMode, Trigger};

/// Where a step lands ([`step_add`] / [`step_move`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Placement {
    /// End of the named track.
    Append { track: String },
    /// Immediately after `after` in whatever track holds it.
    After { after: StepId },
    /// Into a branch's `then`/`else` arm — arm-list membership AND storage
    /// position in one operation (the designer's `insertStepIntoBranchArm`,
    /// adrev A3: branch semantics require the two edits to be atomic).
    /// `after` positions within the arm: `None` appends to the arm list.
    Branch {
        branch: StepId,
        arm: BranchArm,
        after: Option<StepId>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchArm {
    Then,
    Else,
}

impl BranchArm {
    pub fn as_str(self) -> &'static str {
        match self {
            BranchArm::Then => "then",
            BranchArm::Else => "else",
        }
    }
}

/// Precondition failures (D6 outcome 2). Every variant means NO mutation
/// happened; the message names the offending id verbatim so a small model can
/// fix the exact fragment.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EditError {
    #[error("no track named \"{0}\" in this routine")]
    UnknownTrack(String),
    #[error("no step with id \"{0}\" in this routine")]
    UnknownStep(String),
    #[error("step id \"{0}\" already exists — every step id must be unique")]
    DuplicateStepId(String),
    #[error("a track named \"{0}\" already exists")]
    DuplicateTrack(String),
    #[error("step \"{0}\" is not a branch control step")]
    NotABranch(String),
    #[error("step ids must be non-empty")]
    EmptyStepId,
    #[error("track names must be non-empty")]
    EmptyTrackName,
    #[error("patch may not change a step's id — remove and re-add instead")]
    IdChangeRejected,
    #[error(
        "patch may not turn an action step into a control step (or back) — \
         remove and re-add instead"
    )]
    KindChangeRejected,
    #[error("patch does not produce a valid step: {0}")]
    InvalidPatch(String),
    #[error("cannot remove track \"{0}\" — a routine needs at least one track")]
    LastTrack(String),
}

/// What [`step_remove`] / [`track_remove`] scrubbed: branch-arm entries that
/// referenced a removed step, reported so the verb's response can say what
/// was repaired (D1: scrub, don't dangle — the designer calls its scrub
/// load-bearing because recycled ids would silently re-acquire old arm
/// membership).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScrubReport {
    /// `(branch_step_id, arm, removed_step_id)` per scrubbed arm entry.
    pub scrubbed: Vec<(String, &'static str, String)>,
}

/// `s<max+1>` across every `s<n>`-shaped step id in every track — the
/// designer's `nextStepId`, verbatim semantics (ids that don't match the
/// shape are tolerated and ignored for the max). Safe to recycle a freed id
/// BECAUSE removal scrubs arm references (same invariant, same reason).
pub fn next_step_id(def: &RoutineDef) -> String {
    let mut max = 0u64;
    for track in &def.tracks {
        for step in &track.steps {
            if let Some(rest) = step.id().0.strip_prefix('s') {
                if let Ok(n) = rest.parse::<u64>() {
                    if n > max {
                        max = n;
                    }
                }
            }
        }
    }
    format!("s{}", max + 1)
}

fn find_step(def: &RoutineDef, id: &str) -> Option<(usize, usize)> {
    for (ti, track) in def.tracks.iter().enumerate() {
        for (si, step) in track.steps.iter().enumerate() {
            if step.id().0 == id {
                return Some((ti, si));
            }
        }
    }
    None
}

fn step_id_exists(def: &RoutineDef, id: &str) -> bool {
    find_step(def, id).is_some()
}

/// Insert `step` per `placement`. Returns the new def and the inserted step's
/// id (the caller may have had the id server-assigned via [`next_step_id`]
/// before building `step`).
pub fn step_add(
    def: &RoutineDef,
    step: Step,
    placement: &Placement,
) -> Result<(RoutineDef, StepId), EditError> {
    let id = step.id().clone();
    if id.0.is_empty() {
        return Err(EditError::EmptyStepId);
    }
    if step_id_exists(def, &id.0) {
        return Err(EditError::DuplicateStepId(id.0));
    }
    let mut out = def.clone();
    insert_at(&mut out, step, placement)?;
    Ok((out, id))
}

/// The shared splice for [`step_add`] and [`step_move`]'s re-insert half.
fn insert_at(def: &mut RoutineDef, step: Step, placement: &Placement) -> Result<(), EditError> {
    match placement {
        Placement::Append { track } => {
            let t = def
                .tracks
                .iter_mut()
                .find(|t| &t.name == track)
                .ok_or_else(|| EditError::UnknownTrack(track.clone()))?;
            // Append lands BEFORE a trailing `end` control (adrev round 3,
            // 5.5): every step appended after a track's final End is
            // unreachable by construction — the executor stops at the End —
            // and the definition_template ends with one, so a literal
            // template-then-append bootstrap (exactly what a small model
            // does) would build an entirely blocked routine. Nobody means
            // "add an unreachable step"; a trailing End is a terminator,
            // not a position.
            let insert_at = match t.steps.last() {
                Some(Step::Control(c)) if matches!(c.control, Control::End { .. }) => {
                    t.steps.len() - 1
                }
                _ => t.steps.len(),
            };
            t.steps.insert(insert_at, step);
            Ok(())
        }
        Placement::After { after } => {
            let (ti, si) =
                find_step(def, &after.0).ok_or_else(|| EditError::UnknownStep(after.0.clone()))?;
            def.tracks[ti].steps.insert(si + 1, step);
            Ok(())
        }
        Placement::Branch { branch, arm, after } => {
            let (ti, branch_idx) =
                find_step(def, &branch.0).ok_or_else(|| EditError::UnknownStep(branch.0.clone()))?;
            let new_id = step.id().0.clone();
            let track = &mut def.tracks[ti];
            let arm_ids: Vec<String> = match &track.steps[branch_idx] {
                Step::Control(c) => match &c.control {
                    Control::Branch { then, r#else, .. } => match arm {
                        BranchArm::Then => then.iter().map(|s| s.0.clone()).collect(),
                        BranchArm::Else => r#else.iter().map(|s| s.0.clone()).collect(),
                    },
                    _ => return Err(EditError::NotABranch(branch.0.clone())),
                },
                Step::Action(_) => return Err(EditError::NotABranch(branch.0.clone())),
            };

            // Mirror insertStepIntoBranchArm's three positioning cases
            // (defDraft.ts): after==branch id -> front of the arm, storage
            // right after the branch; after names an arm member -> right
            // after it in both the list and storage; None/append -> end of
            // the arm list, storage after the arm's last step present in
            // this track (right after the branch when the arm is empty).
            // Unlike the designer, an `after` that is neither the branch nor
            // an arm member is an error, not a silent append.
            let mut new_arm: Vec<String>;
            let mut insert_idx = branch_idx + 1;
            match after {
                Some(a) if a.0 == branch.0 => {
                    new_arm = Vec::with_capacity(arm_ids.len() + 1);
                    new_arm.push(new_id.clone());
                    new_arm.extend(arm_ids.iter().cloned());
                }
                Some(a) => {
                    let pos = arm_ids
                        .iter()
                        .position(|x| x == &a.0)
                        .ok_or_else(|| EditError::UnknownStep(a.0.clone()))?;
                    new_arm = arm_ids.clone();
                    new_arm.insert(pos + 1, new_id.clone());
                    if let Some(idx) = track.steps.iter().position(|s| s.id().0 == a.0) {
                        insert_idx = idx + 1;
                    }
                }
                None => {
                    new_arm = arm_ids.clone();
                    new_arm.push(new_id.clone());
                    for id in &arm_ids {
                        if let Some(idx) = track.steps.iter().position(|s| &s.id().0 == id) {
                            if idx + 1 > insert_idx {
                                insert_idx = idx + 1;
                            }
                        }
                    }
                }
            }

            track.steps.insert(insert_idx, step);
            // insert_idx > branch_idx always, so the branch's index is stable.
            if let Step::Control(c) = &mut track.steps[branch_idx] {
                if let Control::Branch { then, r#else, .. } = &mut c.control {
                    let target = match arm {
                        BranchArm::Then => then,
                        BranchArm::Else => r#else,
                    };
                    *target = new_arm.into_iter().map(StepId).collect();
                }
            }
            Ok(())
        }
    }
}

/// Shallow-merge `patch`'s keys onto the step's serialized form, then
/// re-deserialize. `params` (one key) therefore replaces wholesale; scalar
/// fields patch individually. The id is immutable through this path, and an
/// action step cannot become a control step (or back, or change its control
/// KIND) — that is a remove+add (D1, adrev 5.6 #6: no discriminator changes
/// through a shallow patch).
pub fn step_update(def: &RoutineDef, step_id: &str, patch: &Value) -> Result<RoutineDef, EditError> {
    let patch_obj = patch
        .as_object()
        .ok_or_else(|| EditError::InvalidPatch("patch must be a JSON object".into()))?;
    let (ti, si) = find_step(def, step_id).ok_or_else(|| EditError::UnknownStep(step_id.into()))?;
    let current = &def.tracks[ti].steps[si];

    if let Some(id_val) = patch_obj.get("id") {
        if id_val != &Value::String(step_id.to_string()) {
            return Err(EditError::IdChangeRejected);
        }
    }
    let is_action = matches!(current, Step::Action(_));
    if is_action && patch_obj.contains_key("control") {
        return Err(EditError::KindChangeRejected);
    }
    if !is_action {
        if patch_obj.contains_key("action") {
            return Err(EditError::KindChangeRejected);
        }
        if let (Some(new_kind), Step::Control(c)) = (patch_obj.get("control"), current) {
            let current_kind = serde_json::to_value(&c.control)
                .ok()
                .and_then(|v| v.get("control").cloned());
            if current_kind.as_ref() != Some(new_kind) {
                return Err(EditError::KindChangeRejected);
            }
        }
    }

    let mut merged = serde_json::to_value(current)
        .map_err(|e| EditError::InvalidPatch(e.to_string()))?;
    let obj = merged
        .as_object_mut()
        .expect("a serialized step is a JSON object");
    for (k, v) in patch_obj {
        if v.is_null() {
            // null clears an optional field (timeout_s, align...) — the
            // serde defaults re-apply on deserialize.
            obj.remove(k);
        } else {
            obj.insert(k.clone(), v.clone());
        }
    }
    let new_step: Step = serde_json::from_value(merged).map_err(|e| {
        // The untagged Step enum's own error is unhelpfully generic; name
        // the step and keep serde's detail.
        EditError::InvalidPatch(format!("step \"{step_id}\": {e}"))
    })?;

    // Reject patch keys serde silently dropped (adrev round 2, both
    // reviewers): the step structs don't deny unknown fields, so a typo like
    // {"timeout": 30} would deserialize unchanged and report applied:true —
    // the exact "reported success, changed nothing" failure D6 outcome 1
    // exists to prevent. Every REAL step field either always serializes or
    // serializes when set to a non-null value (the only skip_serializing_if
    // fields are Options the non-null patch just set), so key-presence in
    // the re-serialized step is a sound acceptance test.
    let reserialized = serde_json::to_value(&new_step)
        .map_err(|e| EditError::InvalidPatch(e.to_string()))?;
    let out_obj = reserialized
        .as_object()
        .expect("a serialized step is a JSON object");
    for (k, v) in patch_obj {
        if !v.is_null() && !out_obj.contains_key(k) {
            return Err(EditError::InvalidPatch(format!(
                "step \"{step_id}\": unknown field \"{k}\" for this step kind — it would be \
                 silently ignored, not applied"
            )));
        }
    }

    let mut out = def.clone();
    out.tracks[ti].steps[si] = new_step;
    Ok(out)
}

/// Remove the step AND scrub every reference to it (spec D1's amended
/// contract: branch/retry references are scrubbed, never left dangling —
/// adrev round 2, both reviewers: a dangling retry target plus a recycled id
/// silently retargets the retry onto an unrelated future step, the same
/// phantom-membership hazard the branch-arm scrub closes). Branch-arm
/// entries are filtered out; a `retry` control step whose TARGET was removed
/// is itself removed (a retry without its target has no meaning), and that
/// removal cascades: the retry's own id is scrubbed from arms and from any
/// retry targeting IT, to a fixpoint. Everything scrubbed is reported.
pub fn step_remove(
    def: &RoutineDef,
    step_id: &str,
) -> Result<(RoutineDef, ScrubReport), EditError> {
    if find_step(def, step_id).is_none() {
        return Err(EditError::UnknownStep(step_id.into()));
    }
    Ok(remove_steps(def, &[step_id.to_string()]))
}

/// The shared removal core for [`step_remove`] / [`track_remove`]: grow the
/// removed set with retry steps whose target is being removed (fixpoint),
/// filter them all out of storage, scrub branch arms.
fn remove_steps(def: &RoutineDef, named: &[String]) -> (RoutineDef, ScrubReport) {
    let mut removed: Vec<String> = named.to_vec();
    let mut report = ScrubReport::default();
    loop {
        let mut grew = false;
        for track in &def.tracks {
            for step in &track.steps {
                if let Step::Control(c) = step {
                    if let Control::Retry { step: target, .. } = &c.control {
                        if removed.contains(&target.0) && !removed.contains(&c.id.0) {
                            report.scrubbed.push((c.id.0.clone(), "retry", target.0.clone()));
                            removed.push(c.id.0.clone());
                            grew = true;
                        }
                    }
                }
            }
        }
        if !grew {
            break;
        }
    }
    let mut out = def.clone();
    for track in &mut out.tracks {
        track.steps.retain(|s| !removed.contains(&s.id().0));
    }
    let arm_report = scrub_arm_refs(&mut out, &removed);
    report.scrubbed.extend(arm_report.scrubbed);
    (out, report)
}

fn scrub_arm_refs(def: &mut RoutineDef, removed: &[String]) -> ScrubReport {
    let mut report = ScrubReport::default();
    for track in &mut def.tracks {
        for step in &mut track.steps {
            if let Step::Control(c) = step {
                let branch_id = c.id.0.clone();
                if let Control::Branch { then, r#else, .. } = &mut c.control {
                    for (arm_name, arm) in [("then", then), ("else", r#else)] {
                        arm.retain(|id| {
                            let hit = removed.contains(&id.0);
                            if hit {
                                report.scrubbed.push((
                                    branch_id.clone(),
                                    arm_name,
                                    id.0.clone(),
                                ));
                            }
                            !hit
                        });
                    }
                }
            }
        }
    }
    report
}

/// Reposition an existing step: splice it out (scrubbing its old arm
/// membership), then re-insert per `placement` (re-establishing arm
/// membership when the placement is a branch arm). One atomic operation —
/// the remove/re-add dance's transient broken-ref states never exist
/// (adrev A6).
pub fn step_move(
    def: &RoutineDef,
    step_id: &str,
    placement: &Placement,
) -> Result<RoutineDef, EditError> {
    let (ti, si) = find_step(def, step_id).ok_or_else(|| EditError::UnknownStep(step_id.into()))?;
    // Validate the destination BEFORE mutating (a bad placement must be
    // outcome-2 no-mutation, and `After`/`Branch` targets are resolved
    // against the def WITHOUT the moving step, so moving after one's own
    // current neighbor still works).
    let mut out = def.clone();
    let step = out.tracks[ti].steps.remove(si);
    scrub_arm_refs(&mut out, &[step_id.to_string()]);
    // Placement targets must not be the moving step itself.
    let self_target = match placement {
        Placement::After { after } => after.0 == step_id,
        Placement::Branch { branch, after, .. } => {
            branch.0 == step_id || after.as_ref().is_some_and(|a| a.0 == step_id)
        }
        Placement::Append { .. } => false,
    };
    if self_target {
        return Err(EditError::UnknownStep(step_id.into()));
    }
    insert_at(&mut out, step, placement)?;
    Ok(out)
}

/// Append a new empty track (designer's `addTrack`).
pub fn track_add(def: &RoutineDef, name: &str) -> Result<RoutineDef, EditError> {
    if name.is_empty() {
        return Err(EditError::EmptyTrackName);
    }
    if def.tracks.iter().any(|t| t.name == name) {
        return Err(EditError::DuplicateTrack(name.into()));
    }
    let mut out = def.clone();
    out.tracks.push(Track {
        name: name.into(),
        steps: Vec::new(),
    });
    Ok(out)
}

/// Remove a track and every step it held, with the same reference scrub as
/// [`step_remove`] applied to the whole batch — branch arms filtered AND
/// retry steps in OTHER tracks whose target lived here are removed too (a
/// retry may only legally target same-track steps, but the scrub walks
/// everything, same posture as the arm scrub). Refuses to remove the last
/// track — a routine with zero tracks is not an editable draft state, it is
/// a shape the parser's consumers never see.
pub fn track_remove(
    def: &RoutineDef,
    name: &str,
) -> Result<(RoutineDef, ScrubReport), EditError> {
    let idx = def
        .tracks
        .iter()
        .position(|t| t.name == name)
        .ok_or_else(|| EditError::UnknownTrack(name.into()))?;
    if def.tracks.len() == 1 {
        return Err(EditError::LastTrack(name.into()));
    }
    let mut out = def.clone();
    let removed_track = out.tracks.remove(idx);
    let removed_ids: Vec<String> = removed_track
        .steps
        .iter()
        .map(|s| s.id().0.clone())
        .collect();
    let (out, report) = remove_steps(&out, &removed_ids);
    Ok((out, report))
}

/// Replace the trigger list wholesale.
pub fn trigger_set(def: &RoutineDef, triggers: Vec<Trigger>) -> RoutineDef {
    let mut out = def.clone();
    out.triggers = triggers;
    out
}

/// Envelope-field patch for [`meta_set`]. `rename` is deliberately absent —
/// rename is identity surgery with store-level semantics (a dedicated verb),
/// not a metadata patch (adrev A5).
#[derive(Debug, Clone, Default, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetaPatch {
    pub transmit_mode: Option<TransmitMode>,
    pub on_interrupted: Option<OnInterrupted>,
    pub inputs: Option<Vec<InputDecl>>,
}

/// Patch envelope fields. Consent-envelope normalization (what happens to
/// `transmit_ack`/`write_ack` when `transmit_mode` changes) is the COMMAND
/// layer's job — this op only writes the requested fields.
pub fn meta_set(def: &RoutineDef, patch: &MetaPatch) -> RoutineDef {
    let mut out = def.clone();
    if let Some(tm) = patch.transmit_mode {
        out.transmit_mode = tm;
    }
    if let Some(oi) = patch.on_interrupted {
        out.on_interrupted = oi;
    }
    if let Some(inputs) = &patch.inputs {
        out.inputs = inputs.clone();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ActionStep, BusyPolicy, ControlStep};
    use serde_json::json;

    fn action(id: &str, name: &str) -> Step {
        Step::Action(ActionStep {
            id: StepId(id.into()),
            action: name.into(),
            params: json!({}),
            timeout_s: None,
            on_radio_busy: BusyPolicy::Wait,
        })
    }

    fn branch(id: &str, on: &str, then: &[&str], r#else: &[&str]) -> Step {
        Step::Control(ControlStep {
            id: StepId(id.into()),
            control: Control::Branch {
                on: on.into(),
                op: None,
                value: None,
                then: then.iter().map(|s| StepId((*s).into())).collect(),
                r#else: r#else.iter().map(|s| StepId((*s).into())).collect(),
            },
        })
    }

    fn def_with(tracks: Vec<Track>) -> RoutineDef {
        RoutineDef {
            routine: "t".into(),
            schema_version: 1,
            transmit_mode: TransmitMode::Attended,
            transmit_ack: None,
            write_ack: None,
            on_interrupted: OnInterrupted::Stay,
            inputs: vec![],
            triggers: vec![Trigger::Manual],
            tracks,
        }
    }

    fn one_track(steps: Vec<Step>) -> RoutineDef {
        def_with(vec![Track {
            name: "track-1".into(),
            steps,
        }])
    }

    fn ids(def: &RoutineDef, track: usize) -> Vec<String> {
        def.tracks[track].steps.iter().map(|s| s.id().0.clone()).collect()
    }

    // ---- next_step_id -------------------------------------------------

    #[test]
    fn next_step_id_is_max_plus_one_ignoring_foreign_shapes() {
        let def = one_track(vec![action("s2", "a"), action("weird", "a"), action("s7", "a")]);
        assert_eq!(next_step_id(&def), "s8");
        assert_eq!(next_step_id(&one_track(vec![])), "s1");
    }

    // ---- step_add -----------------------------------------------------

    #[test]
    fn add_appends_to_named_track() {
        let def = one_track(vec![action("s1", "a")]);
        let (out, id) = step_add(
            &def,
            action("s2", "b"),
            &Placement::Append { track: "track-1".into() },
        )
        .unwrap();
        assert_eq!(id.0, "s2");
        assert_eq!(ids(&out, 0), vec!["s1", "s2"]);
    }

    fn end_step(id: &str) -> Step {
        Step::Control(ControlStep {
            id: StepId(id.into()),
            control: Control::End { failed: false, reason: None },
        })
    }

    #[test]
    fn append_lands_before_a_trailing_end_not_after_it() {
        // The definition_template bootstrap (adrev round 3): track ends with
        // an End control; appending must not create unreachable steps.
        let def = one_track(vec![action("s1", "a"), end_step("s2")]);
        let (out, _) = step_add(
            &def,
            action("s3", "b"),
            &Placement::Append { track: "track-1".into() },
        )
        .unwrap();
        assert_eq!(ids(&out, 0), vec!["s1", "s3", "s2"]);
        // A mid-track End (branch arms may end early) does NOT relocate the
        // append — only a TRAILING End is a terminator-not-a-position.
        let def = one_track(vec![end_step("s1"), action("s2", "a")]);
        let (out, _) = step_add(
            &def,
            action("s3", "b"),
            &Placement::Append { track: "track-1".into() },
        )
        .unwrap();
        assert_eq!(ids(&out, 0), vec!["s1", "s2", "s3"]);
    }

    #[test]
    fn add_after_splices_and_unknown_targets_error() {
        let def = one_track(vec![action("s1", "a"), action("s2", "b")]);
        let (out, _) = step_add(
            &def,
            action("s3", "c"),
            &Placement::After { after: StepId("s1".into()) },
        )
        .unwrap();
        assert_eq!(ids(&out, 0), vec!["s1", "s3", "s2"]);

        assert_eq!(
            step_add(&def, action("s3", "c"), &Placement::After { after: StepId("nope".into()) })
                .unwrap_err(),
            EditError::UnknownStep("nope".into())
        );
        assert_eq!(
            step_add(&def, action("s3", "c"), &Placement::Append { track: "ghost".into() })
                .unwrap_err(),
            EditError::UnknownTrack("ghost".into())
        );
    }

    #[test]
    fn add_duplicate_or_empty_id_errors() {
        let def = one_track(vec![action("s1", "a")]);
        assert_eq!(
            step_add(&def, action("s1", "b"), &Placement::Append { track: "track-1".into() })
                .unwrap_err(),
            EditError::DuplicateStepId("s1".into())
        );
        assert_eq!(
            step_add(&def, action("", "b"), &Placement::Append { track: "track-1".into() })
                .unwrap_err(),
            EditError::EmptyStepId
        );
    }

    #[test]
    fn add_into_branch_arm_updates_list_and_storage_atomically() {
        // s1 branch (then: s2), s2. Insert s3 into then after s2.
        let def = one_track(vec![branch("s1", "x.ok", &["s2"], &[]), action("s2", "a")]);
        let (out, _) = step_add(
            &def,
            action("s3", "b"),
            &Placement::Branch {
                branch: StepId("s1".into()),
                arm: BranchArm::Then,
                after: Some(StepId("s2".into())),
            },
        )
        .unwrap();
        assert_eq!(ids(&out, 0), vec!["s1", "s2", "s3"]);
        match &out.tracks[0].steps[0] {
            Step::Control(c) => match &c.control {
                Control::Branch { then, .. } => {
                    assert_eq!(then, &vec![StepId("s2".into()), StepId("s3".into())]);
                }
                other => panic!("expected branch, got {other:?}"),
            },
            other => panic!("expected control, got {other:?}"),
        }
    }

    #[test]
    fn add_into_arm_front_when_after_is_the_branch_itself() {
        let def = one_track(vec![branch("s1", "x.ok", &["s2"], &[]), action("s2", "a")]);
        let (out, _) = step_add(
            &def,
            action("s3", "b"),
            &Placement::Branch {
                branch: StepId("s1".into()),
                arm: BranchArm::Then,
                after: Some(StepId("s1".into())),
            },
        )
        .unwrap();
        // front of arm list, storage right after the branch
        assert_eq!(ids(&out, 0), vec!["s1", "s3", "s2"]);
        match &out.tracks[0].steps[0] {
            Step::Control(c) => match &c.control {
                Control::Branch { then, .. } => {
                    assert_eq!(then, &vec![StepId("s3".into()), StepId("s2".into())]);
                }
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }

    #[test]
    fn add_into_empty_arm_appends_after_branch() {
        let def = one_track(vec![branch("s1", "x.ok", &[], &[])]);
        let (out, _) = step_add(
            &def,
            action("s2", "a"),
            &Placement::Branch {
                branch: StepId("s1".into()),
                arm: BranchArm::Else,
                after: None,
            },
        )
        .unwrap();
        assert_eq!(ids(&out, 0), vec!["s1", "s2"]);
        match &out.tracks[0].steps[0] {
            Step::Control(c) => match &c.control {
                Control::Branch { r#else, .. } => {
                    assert_eq!(r#else, &vec![StepId("s2".into())]);
                }
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }

    #[test]
    fn add_into_non_branch_or_unknown_arm_member_errors() {
        let def = one_track(vec![action("s1", "a"), branch("s2", "x", &[], &[])]);
        assert_eq!(
            step_add(
                &def,
                action("s3", "b"),
                &Placement::Branch {
                    branch: StepId("s1".into()),
                    arm: BranchArm::Then,
                    after: None,
                },
            )
            .unwrap_err(),
            EditError::NotABranch("s1".into())
        );
        assert_eq!(
            step_add(
                &def,
                action("s3", "b"),
                &Placement::Branch {
                    branch: StepId("s2".into()),
                    arm: BranchArm::Then,
                    after: Some(StepId("s9".into())),
                },
            )
            .unwrap_err(),
            EditError::UnknownStep("s9".into())
        );
    }

    // ---- step_update --------------------------------------------------

    #[test]
    fn update_patches_scalars_and_replaces_params_wholesale() {
        let mut a = action("s1", "radio.connect");
        if let Step::Action(ref mut inner) = a {
            inner.params = json!({"stations": ["W7X"], "bands": ["40m"]});
            inner.timeout_s = Some(60);
        }
        let def = one_track(vec![a]);
        let out = step_update(&def, "s1", &json!({"params": {"stations": ["K7Y"]}})).unwrap();
        match &out.tracks[0].steps[0] {
            Step::Action(inner) => {
                // params replaced wholesale — bands gone
                assert_eq!(inner.params, json!({"stations": ["K7Y"]}));
                // untouched scalar survives
                assert_eq!(inner.timeout_s, Some(60));
            }
            _ => unreachable!(),
        }
        // null clears an optional scalar
        let out = step_update(&def, "s1", &json!({"timeout_s": null})).unwrap();
        match &out.tracks[0].steps[0] {
            Step::Action(inner) => assert_eq!(inner.timeout_s, None),
            _ => unreachable!(),
        }
    }

    #[test]
    fn update_rejects_id_and_kind_changes() {
        let def = one_track(vec![action("s1", "a"), branch("s2", "x", &[], &[])]);
        assert_eq!(
            step_update(&def, "s1", &json!({"id": "s9"})).unwrap_err(),
            EditError::IdChangeRejected
        );
        // same-id in patch is tolerated
        assert!(step_update(&def, "s1", &json!({"id": "s1", "action": "b"})).is_ok());
        assert_eq!(
            step_update(&def, "s1", &json!({"control": "end"})).unwrap_err(),
            EditError::KindChangeRejected
        );
        assert_eq!(
            step_update(&def, "s2", &json!({"action": "a"})).unwrap_err(),
            EditError::KindChangeRejected
        );
        // control KIND change through patch is a kind change too
        assert_eq!(
            step_update(&def, "s2", &json!({"control": "delay", "delay": "+5m"})).unwrap_err(),
            EditError::KindChangeRejected
        );
        // same-kind control patch is fine
        assert!(step_update(&def, "s2", &json!({"control": "branch", "on": "y"})).is_ok());
    }

    #[test]
    fn update_branch_arm_lists_and_bad_shapes() {
        let def = one_track(vec![branch("s1", "x", &["s2"], &[]), action("s2", "a")]);
        let out = step_update(&def, "s1", &json!({"then": ["s2", "s3"]})).unwrap();
        match &out.tracks[0].steps[0] {
            Step::Control(c) => match &c.control {
                Control::Branch { then, .. } => {
                    assert_eq!(then, &vec![StepId("s2".into()), StepId("s3".into())]);
                }
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
        let err = step_update(&def, "s1", &json!({"then": {"bad": true}})).unwrap_err();
        assert!(matches!(err, EditError::InvalidPatch(m) if m.contains("s1")));
        assert_eq!(
            step_update(&def, "ghost", &json!({})).unwrap_err(),
            EditError::UnknownStep("ghost".into())
        );
    }

    // ---- step_remove --------------------------------------------------

    #[test]
    fn remove_scrubs_arm_refs_and_reports() {
        let def = one_track(vec![
            branch("s1", "x", &["s2", "s3"], &["s3"]),
            action("s2", "a"),
            action("s3", "b"),
        ]);
        let (out, report) = step_remove(&def, "s3").unwrap();
        assert_eq!(ids(&out, 0), vec!["s1", "s2"]);
        match &out.tracks[0].steps[0] {
            Step::Control(c) => match &c.control {
                Control::Branch { then, r#else, .. } => {
                    assert_eq!(then, &vec![StepId("s2".into())]);
                    assert!(r#else.is_empty());
                }
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
        assert_eq!(
            report.scrubbed,
            vec![
                ("s1".to_string(), "then", "s3".to_string()),
                ("s1".to_string(), "else", "s3".to_string()),
            ]
        );
        assert_eq!(
            step_remove(&def, "ghost").unwrap_err(),
            EditError::UnknownStep("ghost".into())
        );
    }

    #[test]
    fn removed_id_recycled_by_next_add_carries_no_phantom_arm_membership() {
        // The exact adrev A4 scenario: remove s3 out of then:["s3"], re-add a
        // NEW s3 — the arm must not have silently re-acquired it.
        let def = one_track(vec![branch("s1", "x", &["s3"], &[]), action("s3", "b")]);
        let (out, _) = step_remove(&def, "s3").unwrap();
        let (out, id) = step_add(
            &out,
            action(&next_step_id(&out), "c"),
            &Placement::Append { track: "track-1".into() },
        )
        .unwrap();
        assert_eq!(id.0, "s2"); // recycling max+1 over {s1}
        match &out.tracks[0].steps[0] {
            Step::Control(c) => match &c.control {
                Control::Branch { then, .. } => assert!(then.is_empty()),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }

    // ---- step_move ----------------------------------------------------

    #[test]
    fn move_repositions_within_and_across_tracks() {
        let def = def_with(vec![
            Track {
                name: "a".into(),
                steps: vec![action("s1", "x"), action("s2", "y")],
            },
            Track {
                name: "b".into(),
                steps: vec![action("s3", "z")],
            },
        ]);
        let out = step_move(&def, "s1", &Placement::After { after: StepId("s2".into()) }).unwrap();
        assert_eq!(ids(&out, 0), vec!["s2", "s1"]);
        let out = step_move(&def, "s1", &Placement::Append { track: "b".into() }).unwrap();
        assert_eq!(ids(&out, 0), vec!["s2"]);
        assert_eq!(ids(&out, 1), vec!["s3", "s1"]);
    }

    #[test]
    fn move_into_branch_arm_rebinds_membership_and_scrubs_old() {
        let def = one_track(vec![
            branch("s1", "x", &["s2"], &[]),
            action("s2", "a"),
            action("s3", "b"),
        ]);
        // Move s2 out of the then-arm into the else-arm.
        let out = step_move(
            &def,
            "s2",
            &Placement::Branch {
                branch: StepId("s1".into()),
                arm: BranchArm::Else,
                after: None,
            },
        )
        .unwrap();
        match &out.tracks[0].steps[0] {
            Step::Control(c) => match &c.control {
                Control::Branch { then, r#else, .. } => {
                    assert!(then.is_empty(), "old membership scrubbed");
                    assert_eq!(r#else, &vec![StepId("s2".into())]);
                }
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
        // Self-referential placements error.
        assert!(step_move(&def, "s2", &Placement::After { after: StepId("s2".into()) }).is_err());
    }

    // ---- tracks -------------------------------------------------------

    #[test]
    fn track_add_and_duplicate() {
        let def = one_track(vec![]);
        let out = track_add(&def, "monitor").unwrap();
        assert_eq!(out.tracks.len(), 2);
        assert_eq!(out.tracks[1].name, "monitor");
        assert!(out.tracks[1].steps.is_empty());
        assert_eq!(
            track_add(&out, "monitor").unwrap_err(),
            EditError::DuplicateTrack("monitor".into())
        );
        assert_eq!(track_add(&def, "").unwrap_err(), EditError::EmptyTrackName);
    }

    #[test]
    fn track_remove_scrubs_cross_track_arm_refs_and_keeps_last() {
        let def = def_with(vec![
            Track {
                name: "a".into(),
                steps: vec![branch("s1", "x", &["s9"], &[])],
            },
            Track {
                name: "b".into(),
                steps: vec![action("s9", "z")],
            },
        ]);
        let (out, report) = track_remove(&def, "b").unwrap();
        assert_eq!(out.tracks.len(), 1);
        match &out.tracks[0].steps[0] {
            Step::Control(c) => match &c.control {
                Control::Branch { then, .. } => assert!(then.is_empty()),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
        assert_eq!(report.scrubbed, vec![("s1".to_string(), "then", "s9".to_string())]);
        assert_eq!(
            track_remove(&out, "a").unwrap_err(),
            EditError::LastTrack("a".into())
        );
        assert_eq!(
            track_remove(&def, "ghost").unwrap_err(),
            EditError::UnknownTrack("ghost".into())
        );
    }

    // ---- triggers / meta ---------------------------------------------

    #[test]
    fn trigger_set_replaces_wholesale() {
        let def = one_track(vec![]);
        let out = trigger_set(
            &def,
            vec![Trigger::Schedule {
                every: "2h".into(),
                align: None,
                window: None,
                if_missed: Default::default(),
            }],
        );
        assert_eq!(out.triggers.len(), 1);
        assert!(matches!(out.triggers[0], Trigger::Schedule { .. }));
    }

    #[test]
    fn meta_set_patches_only_named_fields() {
        let def = one_track(vec![]);
        let out = meta_set(
            &def,
            &MetaPatch {
                transmit_mode: Some(TransmitMode::Automatic),
                on_interrupted: None,
                inputs: Some(vec![InputDecl { name: "grid".into(), required: true }]),
            },
        );
        assert_eq!(out.transmit_mode, TransmitMode::Automatic);
        assert_eq!(out.on_interrupted, OnInterrupted::Stay); // untouched
        assert_eq!(out.inputs.len(), 1);
    }

    // ---- retry scrub (adrev round 2) ----------------------------------

    fn retry(id: &str, target: &str) -> Step {
        Step::Control(ControlStep {
            id: StepId(id.into()),
            control: Control::Retry {
                step: StepId(target.into()),
                attempts: 3,
                backoff_s: 0,
            },
        })
    }

    #[test]
    fn remove_scrubs_retry_wrappers_and_cascades_their_arm_membership() {
        // s1 branch (then: [s2]), s2 retry->s3, s3 action. Removing s3 must
        // remove the now-meaningless retry s2 AND scrub s2 out of the arm —
        // otherwise a recycled s3 would be silently re-wrapped by the retry.
        let def = one_track(vec![
            branch("s1", "x", &["s2"], &[]),
            retry("s2", "s3"),
            action("s3", "a"),
        ]);
        let (out, report) = step_remove(&def, "s3").unwrap();
        assert_eq!(ids(&out, 0), vec!["s1"]);
        match &out.tracks[0].steps[0] {
            Step::Control(c) => match &c.control {
                Control::Branch { then, .. } => assert!(then.is_empty()),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
        assert!(report
            .scrubbed
            .contains(&("s2".to_string(), "retry", "s3".to_string())));
        assert!(report
            .scrubbed
            .contains(&("s1".to_string(), "then", "s2".to_string())));
    }

    #[test]
    fn track_remove_scrubs_cross_track_retry_targets() {
        let def = def_with(vec![
            Track {
                name: "a".into(),
                steps: vec![retry("s1", "s9"), action("s2", "x")],
            },
            Track {
                name: "b".into(),
                steps: vec![action("s9", "z")],
            },
        ]);
        let (out, report) = track_remove(&def, "b").unwrap();
        assert_eq!(ids(&out, 0), vec!["s2"], "the retry wrapping a removed target goes too");
        assert!(report
            .scrubbed
            .contains(&("s1".to_string(), "retry", "s9".to_string())));
    }

    // ---- unknown patch keys (adrev round 2) ---------------------------

    #[test]
    fn update_rejects_keys_serde_would_silently_drop() {
        let def = one_track(vec![action("s1", "a"), branch("s2", "x", &[], &[])]);
        // typo'd field on an action step
        let err = step_update(&def, "s1", &json!({"timeout": 30})).unwrap_err();
        assert!(
            matches!(&err, EditError::InvalidPatch(m) if m.contains("timeout")),
            "got {err:?}"
        );
        // a delay payload smuggled onto a branch without changing `control`
        let err = step_update(&def, "s2", &json!({"delay": "+5m"})).unwrap_err();
        assert!(
            matches!(&err, EditError::InvalidPatch(m) if m.contains("delay")),
            "got {err:?}"
        );
        // real fields still patch
        assert!(step_update(&def, "s1", &json!({"timeout_s": 30})).is_ok());
    }

    // ---- purity -------------------------------------------------------

    #[test]
    fn ops_never_mutate_their_input() {
        let def = one_track(vec![branch("s1", "x", &["s2"], &[]), action("s2", "a")]);
        let before = def.clone();
        let _ = step_add(&def, action("s3", "b"), &Placement::Append { track: "track-1".into() });
        let _ = step_update(&def, "s2", &json!({"action": "c"}));
        let _ = step_remove(&def, "s2");
        let _ = step_move(&def, "s2", &Placement::Append { track: "track-1".into() });
        let _ = track_add(&def, "t2");
        let _ = trigger_set(&def, vec![]);
        let _ = meta_set(&def, &MetaPatch::default());
        assert_eq!(def, before);
    }
}
