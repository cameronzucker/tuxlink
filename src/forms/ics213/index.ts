import { Ics213Form } from './Ics213Form';
import { Ics213View } from './Ics213View';
import { registerForm } from '../forms';

registerForm({
  id: 'ICS213_Initial',
  name: 'ICS-213 General Message',
  Form: Ics213Form,
  View: Ics213View,
});
