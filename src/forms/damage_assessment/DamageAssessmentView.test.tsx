import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { DamageAssessmentView } from './DamageAssessmentView';
import type { FormPayload } from '../types';

const PAYLOAD: FormPayload = {
  formId: 'Damage_Assessment_Initial',
  formParameters: {
    xmlFileVersion: '1.0',
    rmsExpressVersion: 'Tuxlink/0.3.0',
    submissionDatetime: '20260531100000',
    sendersCallsign: 'N0CALL',
    gridSquare: 'FM18',
    displayForm: 'Damage_Assessment_Viewer.html',
    replyTemplate: '',
  },
  fields: [
    ['status', 'PRELIMINARY'],
    ['jur', 'Springfield County'],
    ['surarea', 'North District'],
    ['aff1', '12'],
    ['min1', '5'],
    ['maj1', '3'],
    ['des1', '1'],
    ['total1', '21'],
    ['dollar1', '$450,000'],
    ['dollar16', '$450,000'],
    ['comments', 'Initial survey complete; follow-up needed downtown.'],
  ],
};

describe('DamageAssessmentView', () => {
  it('renders header fields and populated category data', () => {
    render(<DamageAssessmentView payload={PAYLOAD} />);
    expect(screen.getByText('PRELIMINARY')).toBeInTheDocument();
    expect(screen.getByText('Springfield County')).toBeInTheDocument();
    expect(screen.getByText('North District')).toBeInTheDocument();
    // Houses category row values
    expect(screen.getByText('12')).toBeInTheDocument();
    expect(screen.getAllByText('$450,000').length).toBeGreaterThan(0);
    expect(screen.getByText(/Initial survey complete/)).toBeInTheDocument();
  });

  it('safely renders field values containing HTML (no innerHTML)', () => {
    const xssPayload: FormPayload = {
      ...PAYLOAD,
      fields: [['comments', '<script>alert(3)</script>']],
    };
    const { container } = render(<DamageAssessmentView payload={xssPayload} />);
    expect(container.innerHTML).not.toContain('<script>alert');
    expect(container.textContent).toContain('<script>alert(3)</script>');
  });
});
