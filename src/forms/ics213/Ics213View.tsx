import type { FormViewProps } from '../forms';
import { fieldValue } from '../types';

export function Ics213View({ payload }: FormViewProps) {
  const f = (id: string) => fieldValue(payload, id) ?? '';
  return (
    <div className="form-view form-view-ics213" data-testid="ics213-view">
      <div className="form-view-header">
        <strong>📋 ICS-213 General Message · v1.0</strong>
      </div>
      {f('isexercise') && (
        <div className="form-view-exercise-marker"><strong>{f('isexercise')}</strong></div>
      )}
      <dl className="form-fields">
        {f('inc_name')   && <><dt>Incident</dt>      <dd>{f('inc_name')}</dd></>}
        <dt>To</dt>        <dd>{f('to_name')}</dd>
        <dt>From</dt>      <dd>{f('fm_name')}</dd>
        <dt>Date</dt>      <dd>{f('mdate')} · Time: {f('mtime')}</dd>
        <dt>Subject</dt>   <dd>{f('subjectline')}</dd>
        <dt>Message</dt>   <dd className="form-message-body"><pre>{f('message')}</pre></dd>
        {f('approved_name') && <><dt>Approved by</dt>  <dd>{f('approved_name')}</dd></>}
        {f('approved_postitle') && <><dt>Position/Title</dt> <dd>{f('approved_postitle')}</dd></>}
      </dl>
    </div>
  );
}
