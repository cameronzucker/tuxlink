import { DamageAssessmentForm } from './DamageAssessmentForm';
import { DamageAssessmentView } from './DamageAssessmentView';
import { registerForm } from '../forms';

registerForm({
  id: 'Damage_Assessment_Initial',
  name: 'Damage Assessment',
  Form: DamageAssessmentForm,
  View: DamageAssessmentView,
});
