// Side-effect imports — register forms at module load via registerForm()
// calls in each form's index.ts; plus the shared forms stylesheet so every
// consumer of this module gets the per-form authoring + read-side rules.
// FormPicker.css is imported separately from FormPicker.tsx (modal-specific).
import './forms.css';
import './ics213';
import './ics309';
import './position';
import './bulletin';
import './damage_assessment';
// CheckInForm intentionally NOT registered pending operator decision on WLE
// schema alignment (2026-06-04 Codex full-diff adrev P1). The CheckInForm code
// exists at src/compose/CheckInForm.tsx + src/forms/checkin/ and is built+tested
// in isolation, but registering it would route picks of `Winlink_Check_In_Initial`
// to a form whose field IDs don't match the WLE template's variable names
// (`MsgTo`, `Organization`, `ContactName`, `MsgSender`, …). Until the operator
// chooses between (a) full WLE schema alignment, (b) simplified-UI-with-WLE-
// payload-mapping at submit, or (c) something else, the picker falls through
// to the existing webview Check-In form (P1 behavior — unchanged from main).
// Re-enable by uncommenting and aligning the field set in checkin.rs::FIELDS.
// import './checkin';

export * from './forms';
export * from './types';
export * from './KeyValueView';
export * from './FormPicker';
