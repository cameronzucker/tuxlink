//! UV-Pro audio transport (SSTV component 1, tuxlink-bcsy).
//!
//! A SECOND RFCOMM channel carrying SBC-encoded audio, multiplexed over the same
//! Bluetooth ACL link as the GAIA control/data channel (`super`). This is what the
//! operator observed as "no connection drop": a 2nd RFCOMM channel, NOT a Bluetooth
//! audio-PROFILE switch. The vendor app does software SSTV encode → 32 kHz mono PCM
//! → SBC → these `AudioData` frames; the radio modulates the decoded audio onto RF.
//!
//! Transport spec reverse-engineered from benlink + the decompiled vendor app
//! (sanctioned RE per the winlink-RE-authoritative-sources rule, same as the GAIA
//! control profile in `super`). Canonical spec: the bd `tuxlink-bcsy` notes; build
//! plan: `docs/superpowers/plans/2026-06-13-sstv-audio-transport.md`.
//!
//! RADIO-1 / ADR 0018: this IS transmit-path code, so the correctness bar applies —
//! a working abort that halts TX (`AudioEnd` + drop the audio socket) and no
//! runaway TX. No tuxlink-added airtime cap / TOT (the operator owns ~5 radios and
//! confirms no such limit; the HTCommander 60s claim is unreliable and not
//! propagated). The agent never transmits; the operator runs the on-air smoke.

pub mod codec;
pub mod framing;
pub mod keying;
pub mod transport;
