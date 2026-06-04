import { PositionView } from './PositionView';
import { PositionFormV2 } from '../../compose/PositionFormV2';
import { registerForm } from '../forms';

// PositionFormV2 — native rebuild with PositionArbiter pre-fill (tuxlink-hnkn P2).
// Replaces the v1 form that was pulled (operator critique 2026-05-31: no GPS
// auto-pull). Receive-side PositionView is unchanged.
registerForm({
  id: 'Position_Report',
  name: 'GPS Position Report',
  Form: PositionFormV2,
  View: PositionView,
});
