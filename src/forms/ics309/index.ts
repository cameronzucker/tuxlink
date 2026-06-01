import { Ics309View } from './Ics309View';
import { registerForm } from '../forms';

// Form field intentionally omitted — the v1 native React Ics309Form was
// pulled because manually typing 30 log entries one-by-one is an emcomm
// error magnet; the form should aggregate from messages_meta over an
// operator-picked time range (operator critique 2026-05-31). Native
// rebuild ships in Phase 2 of the full-parity design
// (docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md).
// Receive-side rendering via Ics309View is preserved.
registerForm({
  id: 'Form-309_Initial',
  name: 'ICS-309 Communications Log',
  View: Ics309View,
});
