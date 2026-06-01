import type { FormViewProps } from '../forms';
import { fieldValue } from '../types';

export function Ics309View({ payload }: FormViewProps) {
  const f = (id: string) => fieldValue(payload, id) ?? '';

  // Collect populated log entries (up to 30)
  const entries: Array<{ n: number; time: string; from: string; to: string; sub: string }> = [];
  for (let i = 1; i <= 30; i++) {
    const time = f(`time${i}`);
    const from = f(`from${i}`);
    const to = f(`to${i}`);
    const sub = f(`sub${i}`);
    if (time || from || to || sub) {
      entries.push({ n: i, time, from, to, sub });
    }
  }

  return (
    <div className="form-view form-view-ics309" data-testid="ics309-view">
      <div className="form-view-header">
        <strong>ICS-309 Communications Log</strong>
      </div>
      <dl className="form-fields">
        <dt>Title</dt>     <dd>{f('title')}</dd>
        {f('page')      && <><dt>Page #</dt>              <dd>{f('page')}</dd></>}
        {f('task')      && <><dt>Task #</dt>              <dd>{f('task')}</dd></>}
        {f('taskname')  && <><dt>Task Name</dt>           <dd>{f('taskname')}</dd></>}
        <dt>Date/Time Prepared</dt> <dd>{f('activitydatetime1')}</dd>
        {f('opper')     && <><dt>Op Period #</dt>         <dd>{f('opper')}</dd></>}
        <dt>Operator</dt>  <dd>{f('opname')}</dd>
        <dt>Station ID</dt><dd>{f('operid')}</dd>
      </dl>

      {entries.length > 0 && (
        <section className="ics309-log-section">
          <h3>Log Entries</h3>
          <table className="ics309-log-table">
            <thead>
              <tr>
                <th>#</th>
                <th>Time</th>
                <th>From</th>
                <th>To</th>
                <th>Subject</th>
              </tr>
            </thead>
            <tbody>
              {entries.map(({ n, time, from, to, sub }) => (
                <tr key={n}>
                  <td>{n}</td>
                  <td>{time}</td>
                  <td>{from}</td>
                  <td>{to}</td>
                  <td>{sub}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}
    </div>
  );
}
