import { useId, type SelectHTMLAttributes } from 'react';

export interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
  label?: string;
}

export function Select({ label, id, className, children, ...rest }: SelectProps) {
  const auto = useId();
  const selId = id ?? auto;
  const cls = className ? `tux-select ${className}` : 'tux-select';
  const select = <select id={selId} className={cls} {...rest}>{children}</select>;
  if (!label) return select;
  return (
    <span className="tux-field-wrap">
      <label htmlFor={selId} className="tux-field-label">{label}</label>
      {select}
    </span>
  );
}
