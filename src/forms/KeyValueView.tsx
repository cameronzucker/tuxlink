import type { FormPayload } from './types';

interface Props {
  payload: FormPayload;
  bodyText: string;
}

export function KeyValueView({ payload, bodyText }: Props) {
  return (
    <div className="form-view form-view-unknown" data-testid="key-value-view">
      <div className="form-view-header">
        <strong>Unknown form: {payload.formId}</strong>
        <p>
          The form's specific renderer is not bundled in this Tuxlink version.
          Below are the raw field/value pairs from the XML payload and the
          sender's plain text rendering.
        </p>
      </div>

      <dl className="form-fields">
        {payload.fields.map(([k, v]) => (
          <div className="form-field-row" key={k}>
            <dt>{k}</dt>
            <dd>{v}</dd>
          </div>
        ))}
      </dl>

      {bodyText && (
        <div className="form-view-body">
          <h4>Message body (sender's text rendering)</h4>
          <pre>{bodyText}</pre>
        </div>
      )}
    </div>
  );
}
