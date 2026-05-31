import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { KeyValueView } from './KeyValueView';
import type { FormPayload } from './types';

const PAYLOAD: FormPayload = {
  formId: 'Unknown_Initial',
  formParameters: {
    xmlFileVersion: '1.0',
    rmsExpressVersion: 'Tuxlink/0.3.0',
    submissionDatetime: '20260530143000',
    sendersCallsign: 'N0CALL',
    gridSquare: 'FM18',
    displayForm: 'Unknown_Initial_Viewer.html',
    replyTemplate: '',
  },
  fields: [
    ['field_a', 'value-a'],
    ['field_b', 'value-b'],
  ],
};

describe('KeyValueView', () => {
  it('renders form-id and unknown-form notice', () => {
    render(<KeyValueView payload={PAYLOAD} bodyText="some plain text" />);
    expect(screen.getByText(/Unknown_Initial/)).toBeInTheDocument();
    expect(screen.getByText(/specific renderer is not bundled/i)).toBeInTheDocument();
  });

  it('renders all field/value pairs', () => {
    render(<KeyValueView payload={PAYLOAD} bodyText="" />);
    expect(screen.getByText('field_a')).toBeInTheDocument();
    expect(screen.getByText('value-a')).toBeInTheDocument();
    expect(screen.getByText('field_b')).toBeInTheDocument();
    expect(screen.getByText('value-b')).toBeInTheDocument();
  });

  it('renders the bodyText (sender plain rendering)', () => {
    render(<KeyValueView payload={PAYLOAD} bodyText="HELLO WORLD" />);
    expect(screen.getByText(/HELLO WORLD/)).toBeInTheDocument();
  });

  it('safely renders field values containing HTML (no innerHTML)', () => {
    const xssPayload: FormPayload = {
      ...PAYLOAD,
      fields: [['evil', '<script>alert(1)</script>']],
    };
    const { container } = render(<KeyValueView payload={xssPayload} bodyText="" />);
    // The literal `<script>` string should be displayed as text, not executed.
    expect(container.innerHTML).not.toContain('<script>alert');
    expect(container.textContent).toContain('<script>alert(1)</script>');
  });
});
