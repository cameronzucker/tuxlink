// Mirror of src-tauri/src/forms/types.rs structures, camelCase.
//
// Field IDs are lowercase per spec §3 wire convention; React form
// components consume the discriminated FieldKind via the FormField
// schema in FormDef.

export type FieldKind = 'text' | 'long_text' | 'date' | 'time' | 'boolean';

export interface FormField {
  id: string;
  label: string;
  kind: FieldKind;
  required: boolean;
  maxLength: number | null;
}

export interface FormDef {
  id: string;
  name: string;
  fields: FormField[];
  subjectTemplate: string;
  bodyTemplate: string;
  displayForm: string;
  replyTemplate: string;
}

export interface FormParameters {
  xmlFileVersion: string;
  rmsExpressVersion: string;
  submissionDatetime: string;
  sendersCallsign: string;
  gridSquare: string;
  displayForm: string;
  replyTemplate: string;
}

export interface FormPayload {
  formId: string;
  formParameters: FormParameters;
  /** [fieldId, value] pairs preserving XML order. */
  fields: [string, string][];
}

/** Convenience accessor: look up a field value by ID. */
export function fieldValue(payload: FormPayload, id: string): string | undefined {
  return payload.fields.find(([k]) => k === id)?.[1];
}
