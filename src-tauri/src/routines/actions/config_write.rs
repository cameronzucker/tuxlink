//! `config.set_ardop` — spec compat-tree rank 5 (routines-round2). The FIRST
//! config-WRITE action, and the exemplar of the `writes_config` consent class.
//!
//! Unlike the rank-1/3 `data.read` config SOURCES (inert reads), this action
//! MUTATES persisted station configuration: it sets the ARDOP transmit
//! `drive_level`. Its descriptor declares **`writes_config: true`** (every
//! other flag `false` — no radio, no transmit, no network). That flag is what
//! the executor's consent-park predicate (group C) keys on: an ATTENDED run
//! parks a `ParkKind::Write` step for operator confirmation BEFORE it runs.
//! **The action itself does NOT re-implement consent** — it just carries the
//! descriptor flag; the park/ack machinery lives in the executor.
//!
//! The write goes through the SHARED, locked
//! [`crate::modem_commands::set_ardop_drive_level`] setter (behind the
//! [`super::ConfigWriteService`] seam), NOT the naive
//! `config_get_ardop → mutate → config_set_ardop` get-then-set pair (a
//! documented lost-update path). The SAME setter backs the MCP `set_ardop`
//! write port, so the routine and agent front-ends share one locked
//! implementation (ADR 0024 P3).
//!
//! Params: `{ "drive_level": u8 }`. A `drive_level > 100` is invalid params,
//! rejected BEFORE any read via the SAME `validate_drive_level` the MCP write
//! path uses. Output: `{"field":"drive_level","old":<u8|null>,"new":<u8>}` —
//! `old` is the pre-mutation value (`null` when previously unset).

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use tuxlink_routines::action::{Action, ActionDescriptor};
use tuxlink_routines::error::StepError;

use super::ConfigWriteService;

const CONFIG_SET_ARDOP: &str = "config.set_ardop";

/// `config.set_ardop` params. `drive_level` is REQUIRED. A value `> 100` is
/// invalid params (rejected up front, mirroring the MCP write path); a value
/// `> 255` fails deserialization (u8 overflow) — both surface as invalid
/// params BEFORE any config read.
#[derive(Debug, Deserialize)]
struct SetArdopParams {
    drive_level: u8,
}

/// `config.set_ardop` — set the ARDOP transmit drive level in station config.
/// `writes_config: true`; every other descriptor flag `false`.
pub struct ConfigSetArdop {
    config_write: Arc<dyn ConfigWriteService>,
}

impl ConfigSetArdop {
    pub fn new(config_write: Arc<dyn ConfigWriteService>) -> Self {
        Self { config_write }
    }
}

#[async_trait]
impl Action for ConfigSetArdop {
    fn descriptor(&self) -> ActionDescriptor {
        ActionDescriptor {
            writes_config: true,
            name: CONFIG_SET_ARDOP,
            label: "Set ARDOP config",
            description:
                "Set the ARDOP transmit drive level (0-100) in station config.",
            needs_radio: false,
            transmits: false,
            needs_internet: false,
        }
    }

    async fn execute(&self, params: Value, cancel: CancellationToken) -> Result<Value, StepError> {
        let parsed: SetArdopParams =
            serde_json::from_value(params).map_err(|e| StepError::Action {
                action: CONFIG_SET_ARDOP.to_string(),
                cause: format!("invalid params: {e}"),
            })?;

        // Validate BEFORE any read — a `drive_level > 100` is rejected up front
        // via the SAME `validate_drive_level` the MCP `set_ardop` write port
        // uses, so the routine and agent front-ends reject identically.
        tuxlink_mcp_core::validate::validate_drive_level(parsed.drive_level).map_err(|e| {
            StepError::Action {
                action: CONFIG_SET_ARDOP.to_string(),
                cause: format!("invalid params: {e}"),
            }
        })?;

        // Locked read-modify-write: `(old, new)` computed inside the config
        // writer lock (no lost update). Cancellation is honored promptly.
        let (old, new) = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(StepError::Cancelled),
            res = self.config_write.set_ardop_drive_level(parsed.drive_level) => res,
        }
        .map_err(|cause| StepError::Action {
            action: CONFIG_SET_ARDOP.to_string(),
            cause,
        })?;

        // `old` is `Option<u8>` → serializes to `null` when previously unset.
        Ok(json!({
            "field": "drive_level",
            "old": old,
            "new": new,
        }))
    }
}

// ============================================================================
// Real seam adapter — MonolithConfigWriteService. Delegates to the SHARED,
// locked `crate::modem_commands::set_ardop_drive_level` free function (which
// reads/writes the process-global config path under the config writer lock),
// so this adapter needs no `AppHandle`. The setter is fully synchronous (it
// holds a std Mutex only across the sync RMW, never across an await), so
// calling it directly in this async fn is safe.
// ============================================================================

pub struct MonolithConfigWriteService;

impl MonolithConfigWriteService {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MonolithConfigWriteService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConfigWriteService for MonolithConfigWriteService {
    async fn set_ardop_drive_level(&self, level: u8) -> Result<(Option<u8>, u8), String> {
        crate::modem_commands::set_ardop_drive_level(level)
    }
}

// ============================================================================
// Tests — trait fake, no config file. The real locked RMW (old/new under the
// lock, absent-field-erases) is tested directly against a temp config dir in
// `modem_commands.rs`; these tests exercise the ACTION's param validation,
// output shape, and consent-class descriptor flag.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- FakeConfigWriteService -------------------------------------------
    // Panics if `set_ardop_drive_level` is called when a test didn't expect it
    // (the invalid-params/cancellation tests rely on this: the reject/cancel
    // happens BEFORE any write).

    type SetFn = dyn Fn(u8) -> Result<(Option<u8>, u8), String> + Send + Sync;

    struct FakeConfigWriteService {
        set: Box<SetFn>,
    }

    impl Default for FakeConfigWriteService {
        fn default() -> Self {
            Self {
                set: Box::new(|_| panic!("set_ardop_drive_level not expected in this test")),
            }
        }
    }

    impl FakeConfigWriteService {
        fn with_set(
            mut self,
            f: impl Fn(u8) -> Result<(Option<u8>, u8), String> + Send + Sync + 'static,
        ) -> Self {
            self.set = Box::new(f);
            self
        }
    }

    #[async_trait]
    impl ConfigWriteService for FakeConfigWriteService {
        async fn set_ardop_drive_level(&self, level: u8) -> Result<(Option<u8>, u8), String> {
            (self.set)(level)
        }
    }

    fn action(fake: FakeConfigWriteService) -> ConfigSetArdop {
        ConfigSetArdop::new(Arc::new(fake))
    }

    // ---- (a) drive_level > 100 is invalid params BEFORE any read -----------

    #[tokio::test]
    async fn drive_level_over_100_is_invalid_params_without_writing() {
        // The default fake panics if the setter is ever called — proving the
        // reject happens BEFORE any read/write, identical to the MCP tool.
        let err = action(FakeConfigWriteService::default())
            .execute(json!({ "drive_level": 101 }), CancellationToken::new())
            .await
            .expect_err("101 exceeds the 0..=100 range");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "config.set_ardop");
                assert!(cause.contains("invalid params"), "got: {cause}");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn drive_level_over_255_is_invalid_params_without_writing() {
        // u8 overflow at deserialization — still rejected before any write.
        let err = action(FakeConfigWriteService::default())
            .execute(json!({ "drive_level": 300 }), CancellationToken::new())
            .await
            .expect_err("300 overflows u8");
        assert!(matches!(err, StepError::Action { .. }));
    }

    #[tokio::test]
    async fn missing_drive_level_is_invalid_params_without_writing() {
        let err = action(FakeConfigWriteService::default())
            .execute(json!({}), CancellationToken::new())
            .await
            .expect_err("a missing drive_level is invalid params");
        assert!(matches!(err, StepError::Action { .. }));
    }

    #[tokio::test]
    async fn drive_level_100_is_accepted_boundary() {
        // Boundary: exactly 100 is valid (the cap is inclusive).
        let out = action(FakeConfigWriteService::default().with_set(|lvl| Ok((Some(10), lvl))))
            .execute(json!({ "drive_level": 100 }), CancellationToken::new())
            .await
            .expect("100 is within the inclusive cap");
        assert_eq!(out["new"], 100);
    }

    // ---- (e) output shape {field, old, new}, old=null when unset -----------

    #[tokio::test]
    async fn output_shape_old_null_when_previously_unset() {
        let out = action(FakeConfigWriteService::default().with_set(|lvl| Ok((None, lvl))))
            .execute(json!({ "drive_level": 42 }), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            out,
            json!({ "field": "drive_level", "old": null, "new": 42 }),
            "old must serialize to null when previously unset"
        );
    }

    #[tokio::test]
    async fn output_shape_old_carries_prior_value() {
        let out = action(FakeConfigWriteService::default().with_set(|lvl| Ok((Some(33), lvl))))
            .execute(json!({ "drive_level": 77 }), CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(
            out,
            json!({ "field": "drive_level", "old": 33, "new": 77 })
        );
    }

    // ---- seam error propagates verbatim as StepError::Action ---------------

    #[tokio::test]
    async fn setter_error_propagates_as_step_action_error() {
        let err = action(
            FakeConfigWriteService::default()
                .with_set(|_| Err("config read failed: wizard not completed".to_string())),
        )
        .execute(json!({ "drive_level": 40 }), CancellationToken::new())
        .await
        .expect_err("a setter failure must surface as a step error");
        match err {
            StepError::Action { action, cause } => {
                assert_eq!(action, "config.set_ardop");
                assert!(cause.contains("wizard not completed"), "got: {cause}");
            }
            other => panic!("expected StepError::Action, got {other:?}"),
        }
    }

    // ---- cancellation before write is prompt -------------------------------

    #[tokio::test]
    async fn pre_cancelled_token_returns_cancelled_without_writing() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = action(FakeConfigWriteService::default())
            .execute(json!({ "drive_level": 40 }), cancel)
            .await
            .expect_err("a pre-cancelled token must not write");
        assert!(matches!(err, StepError::Cancelled));
    }

    // ---- descriptor: writes_config only ------------------------------------

    #[test]
    fn descriptor_writes_config_only() {
        let d = action(FakeConfigWriteService::default()).descriptor();
        assert_eq!(d.name, "config.set_ardop");
        assert_eq!(d.label, "Set ARDOP config");
        assert!(!d.label.is_empty() && !d.description.is_empty());
        assert!(d.writes_config, "config.set_ardop MUST declare writes_config");
        assert!(!d.needs_radio);
        assert!(!d.transmits);
        assert!(!d.needs_internet);
    }
}
