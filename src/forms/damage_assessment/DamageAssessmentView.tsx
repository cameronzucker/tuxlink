import type { FormViewProps } from '../forms';
import { fieldValue } from '../types';

const FIXED_CATEGORIES: Array<{ label: string; n: number }> = [
  { label: 'Houses', n: 1 },
  { label: 'Apartment Complex', n: 2 },
  { label: 'Mobile Homes', n: 3 },
  { label: 'Residential High Rise', n: 4 },
  { label: 'Commercial High Rise', n: 5 },
  { label: 'Public Buildings', n: 6 },
  { label: 'Small Business', n: 7 },
  { label: 'Factories/Industrial', n: 8 },
  { label: 'Roads', n: 9 },
  { label: 'Bridges', n: 10 },
  { label: 'Electrical Distribution', n: 11 },
  { label: 'Schools', n: 12 },
];

interface CategorySectionProps {
  label: string;
  n: number;
  f: (id: string) => string;
}

function CategorySection({ label, n, f }: CategorySectionProps) {
  const aff = f(`aff${n}`);
  const min = f(`min${n}`);
  const maj = f(`maj${n}`);
  const des = f(`des${n}`);
  const total = f(`total${n}`);
  const dollar = f(`dollar${n}`);
  if (!aff && !min && !maj && !des && !total && !dollar) return null;
  return (
    <tr>
      <td>{label}</td>
      <td>{aff}</td>
      <td>{min}</td>
      <td>{maj}</td>
      <td>{des}</td>
      <td>{total}</td>
      <td>{dollar}</td>
    </tr>
  );
}

export function DamageAssessmentView({ payload }: FormViewProps) {
  const f = (id: string) => fieldValue(payload, id) ?? '';

  const otherCategories: Array<{ label: string; n: number }> = [13, 14, 15].map((n) => ({
    label: f(`other${n}`) || `Other ${n}`,
    n,
  }));

  return (
    <div className="form-view form-view-damage-assessment" data-testid="damage-assessment-view">
      <div className="form-view-header">
        <strong>Damage Assessment</strong>
      </div>
      <dl className="form-fields">
        {f('title')     && <><dt>Title</dt>              <dd>{f('title')}</dd></>}
        <dt>Status</dt>        <dd>{f('status')}</dd>
        <dt>Jurisdiction</dt>  <dd>{f('jur')}</dd>
        <dt>Survey Area</dt>   <dd>{f('surarea')}</dd>
        {f('datetime1') && <><dt>Event Date</dt>         <dd>{f('datetime1')}</dd></>}
        {f('date')      && <><dt>Survey Date</dt>        <dd>{f('date')}</dd></>}
        {f('misnum')    && <><dt>Mission/Incident #</dt> <dd>{f('misnum')}</dd></>}
        {f('event')     && <><dt>Event Type</dt>         <dd>{f('event')}</dd></>}
        {f('surteam')   && <><dt>Survey Team</dt>        <dd>{f('surteam')}</dd></>}
      </dl>

      <table className="damage-category-table">
        <thead>
          <tr>
            <th>Category</th>
            <th>Affected</th>
            <th>Minor</th>
            <th>Major</th>
            <th>Totaled</th>
            <th>Total #</th>
            <th>Costs</th>
          </tr>
        </thead>
        <tbody>
          {FIXED_CATEGORIES.map(({ label, n }) => (
            <CategorySection key={n} label={label} n={n} f={f} />
          ))}
          {otherCategories.map(({ label, n }) => (
            <CategorySection key={n} label={label} n={n} f={f} />
          ))}
        </tbody>
      </table>

      {f('dollar16') && (
        <p className="damage-total-cost"><strong>Total Dollar Cost: {f('dollar16')}</strong></p>
      )}
      {f('comments') && (
        <div className="damage-comments">
          <strong>Comments:</strong>
          <pre>{f('comments')}</pre>
        </div>
      )}
    </div>
  );
}
