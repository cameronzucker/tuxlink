// Compose — focused unit tests for the pieces extracted from the
// component scope. The full <Compose /> mount-and-interact tests live
// in the PR's manual smoke (Tauri-runtime-dependent: invoke,
// onCloseRequested, getCurrentWindow), not here. This suite covers
// pure helpers: the ParsedBody → fieldValues conversion that
// handleWebviewSubmit uses to feed `send_webview_form`.
//
// Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md Task 10.

import { describe, expect, it } from 'vitest';
import { parsedBodyToFieldValues } from './Compose';

describe('parsedBodyToFieldValues', () => {
  it('collapses single-value fields to bare strings', () => {
    const out = parsedBodyToFieldValues({
      fields: {
        callsign: ['W6ABC'],
        subject: ['Test'],
      },
      submitter: null,
    });
    expect(out).toEqual({ callsign: 'W6ABC', subject: 'Test' });
  });

  it('joins multi-value fields with newlines', () => {
    // WLE forms use repeated names + checkbox groups; collapsing
    // multi-values via newline preserves the convention forms::parse
    // expects.
    const out = parsedBodyToFieldValues({
      fields: {
        checked_items: ['food', 'water', 'shelter'],
      },
      submitter: null,
    });
    expect(out.checked_items).toBe('food\nwater\nshelter');
  });

  it("strips the synthetic 'Submit' button name", () => {
    // WLE templates POST the submit button's value back as a field
    // named 'Submit'. The backend serializer would just emit it as a
    // <Submit> element in the XML, but it's clearer to strip it at the
    // boundary so the wire format doesn't carry an obviously-meaningless
    // pseudo-field.
    const out = parsedBodyToFieldValues({
      fields: {
        Submit: ['Send'],
        callsign: ['W6ABC'],
      },
      submitter: 'Submit',
    });
    expect(out).not.toHaveProperty('Submit');
    expect(out).toHaveProperty('callsign', 'W6ABC');
  });

  it('returns an empty object for an empty ParsedBody', () => {
    expect(parsedBodyToFieldValues({ fields: {}, submitter: null })).toEqual({});
  });

  it('preserves field order from Object.entries (insertion order for plain objects)', () => {
    // Stability isn't strictly required by the serializer (XML key order
    // is sorted alphabetically inside serialize_catalog_form_xml), but
    // we want consistent test output for the snapshot expectations.
    const out = parsedBodyToFieldValues({
      fields: {
        bravo: ['B'],
        alpha: ['A'],
      },
      submitter: null,
    });
    expect(Object.keys(out)).toEqual(['bravo', 'alpha']);
  });
});
