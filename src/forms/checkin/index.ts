import { CheckInForm } from '../../compose/CheckInForm';
import { CheckInView } from './CheckInView';
import { registerForm } from '../forms';

registerForm({
  id: 'Winlink_Check-In',
  name: 'Winlink Check-In',
  Form: CheckInForm,
  View: CheckInView,
});
