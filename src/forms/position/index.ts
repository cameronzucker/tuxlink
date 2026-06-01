import { PositionForm } from './PositionForm';
import { PositionView } from './PositionView';
import { registerForm } from '../forms';

registerForm({
  id: 'Position_Report',
  name: 'GPS Position Report',
  Form: PositionForm,
  View: PositionView,
});
