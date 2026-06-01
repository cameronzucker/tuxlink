import { PositionView } from './PositionView';
import { registerForm } from '../forms';

// Form field intentionally omitted — the v1 native React PositionForm was
// pulled because it had no GPS auto-pull from PositionArbiter (operator
// critique 2026-05-31). The native rebuild ships in Phase 2 of the
// full-parity design (docs/superpowers/specs/2026-05-31-html-forms-full-parity-design.md).
// Receive-side rendering via PositionView is preserved.
registerForm({
  id: 'Position_Report',
  name: 'GPS Position Report',
  View: PositionView,
});
