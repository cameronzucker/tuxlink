import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Ics309View } from './Ics309View';
import type { FormPayload } from '../types';

const PAYLOAD: FormPayload = {
  formId: 'Form-309_Initial',
  formParameters: {
    xmlFileVersion: '1.0',
    rmsExpressVersion: 'Tuxlink/0.3.0',
    submissionDatetime: '20260531090000',
    sendersCallsign: 'W1AW',
    gridSquare: 'FN31',
    displayForm: 'Form-309_Viewer.html',
    replyTemplate: '',
  },
  fields: [
    ['title', 'Alpha Net'],
    ['opname', 'W1AW'],
    ['operid', 'W1AW-1'],
    ['activitydatetime1', '2026-05-31 09:00Z'],
    ['time1', '09:05Z'],
    ['from1', 'W1AW'],
    ['to1', 'KD9XYZ'],
    ['sub1', 'Welfare traffic for Smith family'],
  ],
};

describe('Ics309View', () => {
  it('renders header and log entry fields', () => {
    render(<Ics309View payload={PAYLOAD} />);
    expect(screen.getByText('Alpha Net')).toBeInTheDocument();
    // W1AW appears in both the header dl and the log table — use getAllByText
    expect(screen.getAllByText('W1AW').length).toBeGreaterThan(0);
    expect(screen.getByText('09:05Z')).toBeInTheDocument();
    expect(screen.getByText('Welfare traffic for Smith family')).toBeInTheDocument();
  });

  it('safely renders field values containing HTML (no innerHTML)', () => {
    const xssPayload: FormPayload = {
      ...PAYLOAD,
      fields: [['sub1', '<script>alert(1)</script>']],
    };
    const { container } = render(<Ics309View payload={xssPayload} />);
    expect(container.innerHTML).not.toContain('<script>alert');
    expect(container.textContent).toContain('<script>alert(1)</script>');
  });
});
