import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { BulletinView } from './BulletinView';
import type { FormPayload } from '../types';

const PAYLOAD: FormPayload = {
  formId: 'Bulletin_Initial',
  formParameters: {
    xmlFileVersion: '1.0',
    rmsExpressVersion: 'Tuxlink/0.3.0',
    submissionDatetime: '20260531090000',
    sendersCallsign: 'W1AW',
    gridSquare: 'FN31',
    displayForm: 'Bulletin Viewer.html',
    replyTemplate: '',
  },
  fields: [
    ['level', 'ROUTINE'],
    ['subjectline', 'Net schedule update'],
    ['bullnr', '42'],
    ['name', 'ALL'],
    ['from_name', 'W1AW'],
    ['activitydatetime1', '2026-05-31 09:00Z'],
    ['message', 'Net moved to 0930 local.'],
  ],
};

describe('BulletinView', () => {
  it('renders all bulletin fields', () => {
    render(<BulletinView payload={PAYLOAD} />);
    expect(screen.getByText(/ROUTINE/)).toBeInTheDocument();
    expect(screen.getByText('Net schedule update')).toBeInTheDocument();
    expect(screen.getByText('ALL')).toBeInTheDocument();
    expect(screen.getByText('W1AW')).toBeInTheDocument();
    expect(screen.getByText('Net moved to 0930 local.')).toBeInTheDocument();
  });

  it('safely renders field values containing HTML (no innerHTML)', () => {
    const xssPayload: FormPayload = {
      ...PAYLOAD,
      fields: [['message', '<script>alert(2)</script>']],
    };
    const { container } = render(<BulletinView payload={xssPayload} />);
    expect(container.innerHTML).not.toContain('<script>alert');
    expect(container.textContent).toContain('<script>alert(2)</script>');
  });
});
