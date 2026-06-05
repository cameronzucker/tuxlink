import type { FormViewProps } from '../forms';
import { fieldValue } from '../types';

export function CheckInView({ payload }: FormViewProps) {
  const f = (id: string) => fieldValue(payload, id) ?? '';
  return (
    <div className="form-view form-view-checkin" data-testid="checkin-view">
      <div className="form-view-header">
        <strong>Winlink Check-In — {f('tactical_call')}</strong>
      </div>
      <dl className="form-fields">
        <dt>Tactical Call</dt>   <dd>{f('tactical_call')}</dd>
        {f('op_name')   && <><dt>Operator Name</dt> <dd>{f('op_name')}</dd></>}
        {f('group_net') && <><dt>Group / Net</dt>   <dd>{f('group_net')}</dd></>}
        <dt>Status</dt>          <dd>{f('status')}</dd>
        {f('grid')      && <><dt>Grid Square</dt>   <dd>{f('grid')}</dd></>}
        {f('comments')  && <><dt>Comments</dt>      <dd>{f('comments')}</dd></>}
        {f('initials')  && <><dt>Initials</dt>      <dd>{f('initials')}</dd></>}
      </dl>
    </div>
  );
}
