import type { FormViewProps } from '../forms';
import { fieldValue } from '../types';

/** Receive-side view for Winlink Check-In messages. Mirrors the body-template
 *  sections in src-tauri/src/forms/templates/checkin.rs::BODY_TEMPLATE (and
 *  the WLE `Winlink Check-in.txt` source). Renders empty-string fields as
 *  conditional rows so a sparse Check-In doesn't show a wall of blanks. */
export function CheckInView({ payload }: FormViewProps) {
  const f = (id: string) => fieldValue(payload, id) ?? '';
  return (
    <div className="form-view form-view-checkin" data-testid="checkin-view">
      <div className="form-view-header">
        <strong>
          Winlink Check-In — {f('msgsender')}
          {f('newsubject') && ` — ${f('newsubject')}`}
        </strong>
      </div>

      <dl className="form-fields">
        {/* 0. HEADER */}
        <dt>Organization</dt>   <dd>{f('organization')}</dd>
        {f('newsubject')  && <><dt>Subject</dt>          <dd>{f('newsubject')}</dd></>}
        {f('exercise_id') && <><dt>Exercise ID</dt>      <dd>{f('exercise_id')}</dd></>}

        {/* 1. STATION */}
        <dt>Date/Time</dt>      <dd>{f('datetime')}</dd>
        <dt>To</dt>             <dd>{f('msgto')}</dd>
        <dt>From</dt>           <dd>{f('msgsender')}</dd>
        <dt>Station Contact</dt><dd>{f('contactname')}</dd>
        {f('assigned')    && <><dt>Initial Operators</dt><dd>{f('assigned')}</dd></>}

        {/* 2. SESSION */}
        <dt>Type</dt>           <dd>{f('status')}</dd>
        <dt>Service</dt>        <dd>{f('service')}</dd>
        <dt>Band</dt>           <dd>{f('band')}</dd>
        <dt>Session</dt>        <dd>{f('session')}</dd>

        {/* 3. LOCATION */}
        {f('location') && <><dt>Location</dt>       <dd>{f('location')}</dd></>}
        {f('grid')     && <><dt>Grid Square</dt>    <dd>{f('grid')}</dd></>}
        {(f('maplat') && f('maplon')) && (
          <><dt>Lat / Lon</dt> <dd>{f('maplat')}, {f('maplon')}</dd></>
        )}
        {f('mgrs')     && <><dt>MGRS</dt>           <dd>{f('mgrs')}</dd></>}
        {f('locationsource') && <><dt>Location Source</dt><dd>{f('locationsource')}</dd></>}

        {/* 4. COMMENTS */}
        {f('comments') && <><dt>Comments</dt>       <dd>{f('comments')}</dd></>}
      </dl>
    </div>
  );
}
