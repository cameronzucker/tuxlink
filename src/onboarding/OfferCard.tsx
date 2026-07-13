// OfferCard — the first-run "want a quick tour?" prompt (tuxlink-10bkw Task 6).
//
// Renders only while `hints.active === {kind:'offer'}`. Deliberately NOT
// overlay-active (HintProvider.isOverlayCapturing excludes 'offer' on
// purpose — see HintProvider.tsx's file header): it never blocks typing and
// never steals focus. Fixed bottom-right, `role="status"` (a passive
// announcement, not a modal dialog).

import { useHints } from './HintProvider';
import './HintOverlay.css';

export function OfferCard() {
  const hints = useHints();
  if (hints.active?.kind !== 'offer') return null;

  return (
    <div className="hint-offer-card" role="status" data-testid="hint-offer-card">
      <p className="hint-offer-card__body">Want a 60-second tour of the shell?</p>
      <div className="hint-offer-card__actions">
        <button
          type="button"
          className="hint-overlay__btn hint-overlay__btn--primary"
          data-testid="hint-offer-start"
          onClick={hints.startTour}
        >
          Start tour
        </button>
        <button
          type="button"
          className="hint-overlay__btn hint-overlay__btn--ghost"
          data-testid="hint-offer-decline"
          onClick={hints.declineOffer}
        >
          No thanks
        </button>
      </div>
    </div>
  );
}
