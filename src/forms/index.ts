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
// CheckInForm — WLE-aligned per bd tuxlink-4ai0 (rebuilt from the simplified
// 7-field design that landed disabled in PR #392). FIELDS array in
// src-tauri/src/forms/templates/checkin.rs now matches the bundled
// Winlink_Check_In_Initial.html template's <var> placeholders, so received
// messages render in the WLE viewer + CMS / other Winlink clients recognize
// the form as a standard Check-In.
import './checkin';

export * from './forms';
export * from './types';
export * from './KeyValueView';
export * from './FormPicker';
