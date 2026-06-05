import { Ics309FormV2 } from '../../compose/Ics309FormV2';
import { Ics309View } from './Ics309View';
import { registerForm } from '../forms';

// tuxlink-hnkn P2 Task 2: native ICS-309 Comms Log compose form.
// Aggregates messages_meta over an operator-picked time range; CSV + PDF export.
// Receive-side rendering via Ics309View is preserved.
registerForm({
  id: 'Form-309_Initial',
  name: 'ICS-309 Communications Log',
  Form: Ics309FormV2,
  View: Ics309View,
});
