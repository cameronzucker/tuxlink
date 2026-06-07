import {
  DRAFTS_CHANGED_EVENT,
  listDraftIds,
  loadDraft,
  splitAddrs,
  type DraftData,
} from '../compose/useDraft';
import type { MessageMeta } from './types';

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

export function listDraftMessages(): MessageMeta[] {
  return listDraftIds()
    .map((id) => loadDraft(id))
    .filter((draft): draft is DraftData => draft !== null)
    .map(draftToMessageMeta);
}
