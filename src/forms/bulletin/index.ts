import { BulletinForm } from './BulletinForm';
import { BulletinView } from './BulletinView';
import { registerForm } from '../forms';

registerForm({
  id: 'Bulletin_Initial',
  name: 'Bulletin',
  Form: BulletinForm,
  View: BulletinView,
});
