import { Ics309Form } from './Ics309Form';
import { Ics309View } from './Ics309View';
import { registerForm } from '../forms';

registerForm({
  id: 'Form-309_Initial',
  name: 'ICS-309 Communications Log',
  Form: Ics309Form,
  View: Ics309View,
});
