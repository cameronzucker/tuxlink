import { useId, type InputHTMLAttributes } from 'react';

export interface FieldProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string;
}

export function Field({ label, id, className, ...rest }: FieldProps) {
  const auto = useId();
  const fieldId = id ?? auto;
  const cls = className ? `tux-field ${className}` : 'tux-field';
  const input = <input id={fieldId} className={cls} {...rest} />;
  if (!label) return input;
  return (
    <span className="tux-field-wrap">
      <label htmlFor={fieldId} className="tux-field-label">{label}</label>
      {input}
    </span>
  );
}
