import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { PositionView } from './PositionView';
import type { FormPayload } from '../types';

const PAYLOAD: FormPayload = {
  formId: 'Position_Report',
  formParameters: {
    xmlFileVersion: '1.0',
    rmsExpressVersion: 'Tuxlink/0.3.0',
    submissionDatetime: '20260531140000',
    sendersCallsign: 'N0CALL',
    gridSquare: 'FM18',
    displayForm: 'GPS Position Report.html',
    replyTemplate: '',
  },
  fields: [
    ['thetime', '14:00Z'],
    ['lat', '38.889484'],
    ['lon', '-77.035278'],
    ['message', 'EOC Alpha staging area'],
  ],
};

describe('PositionView', () => {
  it('renders position fields', () => {
    render(<PositionView payload={PAYLOAD} />);
    expect(screen.getByText('14:00Z')).toBeInTheDocument();
    expect(screen.getByText('38.889484')).toBeInTheDocument();
    expect(screen.getByText('-77.035278')).toBeInTheDocument();
    expect(screen.getByText('EOC Alpha staging area')).toBeInTheDocument();
  });

  it('safely renders field values containing HTML (no innerHTML)', () => {
    const xssPayload: FormPayload = {
      ...PAYLOAD,
      fields: [['message', '<script>alert(1)</script>']],
    };
    const { container } = render(<PositionView payload={xssPayload} />);
    expect(container.innerHTML).not.toContain('<script>alert');
    expect(container.textContent).toContain('<script>alert(1)</script>');
  });
});
