import { DamageAssessmentView } from './DamageAssessmentView';
import { registerForm } from '../forms';

// Form field intentionally omitted — Damage Assessment moves to the
// Phase 1 webview-default rendering path (operator critique 2026-05-31).
// If we later add "active incident" state with metadata to pre-fill, the
// form may be elevated back to native in a future phase. Receive-side
// rendering via DamageAssessmentView is preserved.
registerForm({
  id: 'Damage_Assessment_Initial',
  name: 'Damage Assessment',
  View: DamageAssessmentView,
});
