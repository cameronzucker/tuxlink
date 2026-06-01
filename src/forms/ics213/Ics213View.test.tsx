import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Ics213View } from './Ics213View';
import type { FormPayload } from '../types';

const PAYLOAD: FormPayload = {
  formId: 'ICS213_Initial',
  formParameters: {
    xmlFileVersion: '1.0',
    rmsExpressVersion: 'Tuxlink/0.3.0',
    submissionDatetime: '20260530143000',
    sendersCallsign: 'N0CALL',
    gridSquare: 'FM18',
    displayForm: 'ICS213_Initial_Viewer.html',
    replyTemplate: 'ICS213_SendReply.0',
  },
  fields: [
    ['inc_name', 'WALDO'],
    ['to_name', 'JOHN'],
    ['fm_name', 'JANE'],
    ['subjectline', 'TEST'],
    ['mdate', '2026-05-30'],
    ['mtime', '14:30Z'],
    ['message', 'Need bandages.'],
    ['isexercise', '** THIS IS AN EXERCISE **'],
  ],
};

describe('Ics213View', () => {
  it('renders all labeled field values', () => {
    render(<Ics213View payload={PAYLOAD} />);
    expect(screen.getByText('WALDO')).toBeInTheDocument();
    expect(screen.getByText('JOHN')).toBeInTheDocument();
    expect(screen.getByText('JANE')).toBeInTheDocument();
    expect(screen.getByText('TEST')).toBeInTheDocument();
    expect(screen.getByText(/Need bandages/)).toBeInTheDocument();
  });

  it('shows the IsExercise marker when set', () => {
    render(<Ics213View payload={PAYLOAD} />);
    expect(screen.getByText(/THIS IS AN EXERCISE/)).toBeInTheDocument();
  });

  it('safely renders field values containing HTML (no innerHTML)', () => {
    const xssPayload: FormPayload = {
      ...PAYLOAD,
      fields: [['message', '<script>alert(1)</script>']],
    };
    const { container } = render(<Ics213View payload={xssPayload} />);
    expect(container.innerHTML).not.toContain('<script>alert');
    expect(container.textContent).toContain('<script>alert(1)</script>');
  });
});
