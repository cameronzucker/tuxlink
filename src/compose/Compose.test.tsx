// Compose — focused unit tests for the pieces extracted from the
// component scope. The full <Compose /> mount-and-interact tests live
// in the PR's manual smoke (Tauri-runtime-dependent: invoke,
// onCloseRequested, getCurrentWindow), not here. This suite covers
// pure helpers: the ParsedBody → fieldValues conversion that
// handleWebviewSubmit uses to feed `send_webview_form`.
//
// Plan: docs/superpowers/plans/2026-06-01-html-forms-p1-webview-infra.md Task 10.

import { describe, expect, it } from 'vitest';
import {
  closePromptShape,
  isSaveDraftAvailable,
  parsedBodyToFieldValues,
} from './Compose';

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

// ============================================================================
// P1.1 (2026-06-04 Codex adrev): Save Draft must NOT silently lose webview
// form contents. closePromptShape + isSaveDraftAvailable encode the dialog
// + toolbar conditions; the rendering side reads from these helpers.
// ============================================================================

describe('isSaveDraftAvailable', () => {
  it('is true for plain, pick, and form modes', () => {
    expect(isSaveDraftAvailable('plain')).toBe(true);
    expect(isSaveDraftAvailable('pick')).toBe(true);
    expect(isSaveDraftAvailable('form')).toBe(true);
  });

  it('is false for webview-form mode (Codex adrev P1.1)', () => {
    // In webview-form mode the field values live inside the embedded
    // child webview; Compose has no IPC introspection into them. Save
    // Draft would persist only the formId metadata while silently
    // losing every typed field value — the exact UX trap Codex
    // flagged. Hide the affordance entirely.
    expect(isSaveDraftAvailable('webview-form')).toBe(false);
  });
});

describe('closePromptShape', () => {
  it('returns the Save / Discard / Cancel triad for plain mode', () => {
    const shape = closePromptShape('plain', 'close');
    expect(shape.primary).toBe('This draft has unsaved changes.');
    expect(shape.sub).toBeUndefined();
    expect(shape.buttons).toEqual(['save', 'discard', 'cancel']);
  });

  it('returns the switch-to-form variant when transitioning from plain to form picker', () => {
    const shape = closePromptShape('plain', 'switch-to-form');
    expect(shape.primary).toBe('Save changes before switching to a form?');
    expect(shape.buttons).toEqual(['save', 'discard', 'cancel']);
  });

  it('returns the Save / Discard / Cancel triad for native form mode', () => {
    // Native React forms own their field values via setFormMode; Save
    // Draft can capture them. The full triad applies.
    const shape = closePromptShape('form', 'close');
    expect(shape.buttons).toEqual(['save', 'discard', 'cancel']);
  });

  it('omits Save and surfaces an explainer in webview-form mode (Codex adrev P1.1)', () => {
    // The key regression test for P1.1: in webview-form mode the
    // close-dialog must NOT offer Save Draft, must explain why, and
    // must offer Discard + Cancel only. The operator can Cancel back
    // to the form and press its Send button — that's the only path
    // that preserves the form contents.
    const shape = closePromptShape('webview-form', 'close');
    expect(shape.buttons).toEqual(['discard', 'cancel']);
    expect(shape.buttons).not.toContain('save');
    expect(shape.primary).toMatch(/can't be saved as a draft/i);
    expect(shape.sub).toMatch(/embedded form window/i);
    expect(shape.sub).toMatch(/Cancel.*Send button/i);
  });

  it('webview-form mode ignores the action — same shape for close + switch-to-form', () => {
    const closeShape = closePromptShape('webview-form', 'close');
    const switchShape = closePromptShape('webview-form', 'switch-to-form');
    expect(closeShape).toEqual(switchShape);
  });
});
