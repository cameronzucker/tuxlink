import type { ComponentType } from 'react';
import type { FormPayload } from './types';

/** Form authoring (compose-side) component contract. */
export interface FormComposeProps {
  initialValues?: Record<string, string>;
  /** Called when user submits valid form. */
  onSubmit: (values: Record<string, string>) => void;
  onCancel: () => void;
}

/** Form viewing (read-side) component contract. */
export interface FormViewProps {
  payload: FormPayload;
}

/** Registry entry for a single bundled form. */
export interface FormRegistryEntry {
  id: string;
  name: string;
  Form: ComponentType<FormComposeProps>;
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
