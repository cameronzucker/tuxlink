import type { FormViewProps } from '../forms';
import { fieldValue } from '../types';

export function BulletinView({ payload }: FormViewProps) {
  const f = (id: string) => fieldValue(payload, id) ?? '';
  return (
    <div className="form-view form-view-bulletin" data-testid="bulletin-view">
      <div className="form-view-header">
        <strong>Bulletin #{f('bullnr')} — {f('level')}</strong>
      </div>
      <dl className="form-fields">
        <dt>Subject</dt>         <dd>{f('subjectline')}</dd>
        {f('title')           && <><dt>Title</dt>          <dd>{f('title')}</dd></>}
        <dt>For</dt>             <dd>{f('name')}</dd>
        <dt>From</dt>            <dd>{f('from_name')}</dd>
        <dt>Date/Time</dt>       <dd>{f('activitydatetime1')}</dd>
        <dt>Message</dt>         <dd className="form-message-body"><pre>{f('message')}</pre></dd>
      </dl>
    </div>
  );
}
