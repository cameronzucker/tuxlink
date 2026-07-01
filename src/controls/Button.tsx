import type { ButtonHTMLAttributes } from 'react';

export type ButtonTone = 'neutral' | 'primary' | 'danger';
export type ButtonEmphasis = 'solid' | 'soft' | 'outline';
export type ButtonSize = 'xs' | 'sm' | 'md';

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  tone?: ButtonTone;
  emphasis?: ButtonEmphasis;
  size?: ButtonSize;
}

export function Button({
  tone = 'neutral', emphasis = 'solid', size = 'md',
  className, type = 'button', ...rest
}: ButtonProps) {
  const cls = `tux-btn tux-btn--${tone} tux-btn--${emphasis} tux-btn--${size}`;
  return <button type={type} className={className ? `${cls} ${className}` : cls} {...rest} />;
}
