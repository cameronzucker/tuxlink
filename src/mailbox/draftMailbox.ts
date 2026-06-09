import {
  DRAFTS_CHANGED_EVENT,
  listDraftIds,
  loadDraft,
  splitAddrs,
  type DraftData,
} from '../compose/useDraft';
import type { MessageMeta, ParsedMessage } from './types';

export { DRAFTS_CHANGED_EVENT };

function bodyPreview(body: string): string | undefined {
  const text = body.trim().replace(/\s+/g, ' ');
  if (!text) return undefined;
  return text.length > 120 ? `${text.slice(0, 117)}...` : text;
}

export function draftToMessageMeta(draft: DraftData): MessageMeta {
  return {
    id: draft.draftId,
    subject: draft.subject.trim() || '(No subject)',
    from: 'Draft',
    to: splitAddrs(draft.to),
    date: draft.savedAt,
    unread: false,
    bodySize: draft.body.length,
    hasAttachments: false,
    preview: bodyPreview(draft.body),
    folder: 'drafts',
  };
}

function blankFormParameters(formId: string) {
  return {
    xmlFileVersion: '',
    rmsExpressVersion: 'Tuxlink draft',
    submissionDatetime: '',
    sendersCallsign: '',
    gridSquare: '',
    displayForm: `${formId}_Viewer.html`,
    replyTemplate: '',
  };
}

export function draftToParsedMessage(draft: DraftData): ParsedMessage {
  const formPayload = draft.formId
    ? {
        formId: draft.formId,
        formParameters: blankFormParameters(draft.formId),
        fields: Object.entries(draft.formFields ?? {}),
      }
    : null;

  return {
    id: draft.draftId,
    subject: draft.subject.trim() || '(No subject)',
    from: 'Draft',
    to: splitAddrs(draft.to),
    cc: splitAddrs(draft.cc ?? ''),
    date: draft.savedAt,
    body: draft.body,
    attachments: [],
    isForm: Boolean(draft.formId),
    routing: null,
    formId: draft.formId ?? null,
    formPayload,
  };
}

export function listDraftMessages(): MessageMeta[] {
  return listDraftIds()
    .map((id) => loadDraft(id))
    .filter((draft): draft is DraftData => draft !== null)
    .map(draftToMessageMeta);
}
