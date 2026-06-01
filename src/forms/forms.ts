import type { ComponentType } from 'react';
import type { FormPayload } from './types';

/** Form authoring (compose-side) component contract. */
export interface FormComposeProps {
  initialValues?: Record<string, string>;
  /** Called whenever a form field changes — used by hosts (e.g., Compose)
   *  to lift form values into draft autosave state so they survive close
   *  and reopen. */
  onChange?: (values: Record<string, string>) => void;
  /** Called when user submits valid form. */
  onSubmit: (values: Record<string, string>) => void;
  onCancel: () => void;
}

/** Form viewing (read-side) component contract. */
export interface FormViewProps {
  payload: FormPayload;
}

/** Registry entry for a single bundled form.
 *
 * `Form` is OPTIONAL: some bundled forms register a `View` only (receive-side
 * dispatch / display) without a compose-side authoring component. P0 examples:
 * Position, ICS-309, Damage Assessment — their fill-in UX was pulled per the
 * full-parity design and is being rebuilt on native rails in a later phase.
 * P1+: webview-only forms that delegate authoring to an embedded webview.
 *
 * Picker callers should use `composableForms()` to scope to entries that have
 * a `Form` component. `lookupForm(id)` still resolves view-only entries so
 * the receive-side dispatch continues to work. */
export interface FormRegistryEntry {
  id: string;
  name: string;
  Form?: ComponentType<FormComposeProps>;
  View: ComponentType<FormViewProps>;
}

/** Lookup-by-id registry. Populated by the per-form module imports below. */
const REGISTRY: Map<string, FormRegistryEntry> = new Map();

export function registerForm(entry: FormRegistryEntry): void {
  REGISTRY.set(entry.id, entry);
}

export function lookupForm(id: string): FormRegistryEntry | undefined {
  return REGISTRY.get(id);
}

export function allForms(): FormRegistryEntry[] {
  return Array.from(REGISTRY.values());
}

/** Picker-scope view of the registry: only entries that carry a compose-side
 *  `Form` component. The return type narrows `Form` to non-undefined so
 *  callers can use `entry.Form` directly without a null-check. */
export function composableForms(): Array<
  FormRegistryEntry & { Form: NonNullable<FormRegistryEntry['Form']> }
> {
  return Array.from(REGISTRY.values()).filter(
    (e): e is FormRegistryEntry & { Form: NonNullable<FormRegistryEntry['Form']> } =>
      e.Form !== undefined,
  );
}
