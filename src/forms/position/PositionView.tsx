import type { FormViewProps } from '../forms';
import { fieldValue } from '../types';

export function PositionView({ payload }: FormViewProps) {
  const f = (id: string) => fieldValue(payload, id) ?? '';
  return (
    <div className="form-view form-view-position" data-testid="position-view">
      <div className="form-view-header">
        <strong>GPS Position Report</strong>
      </div>
      <dl className="form-fields">
        <dt>Time</dt>      <dd>{f('thetime')}</dd>
        <dt>Latitude</dt>  <dd>{f('lat')}</dd>
        <dt>Longitude</dt> <dd>{f('lon')}</dd>
        {f('message') && <><dt>Comment</dt> <dd>{f('message')}</dd></>}
      </dl>
    </div>
  );
}
