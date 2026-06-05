import { useEffect, useId, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { FormComposeProps } from '../forms/forms';
import { listSlots, upsertSlot, deleteSlot, type FormDraftSlot } from './FormDraftLibrary';
import './CheckInForm.css';

interface PositionFix {
  grid: string | null;
  source: string;
  fresh: boolean;
}

interface Config {
  callsign?: string;
}

type CheckInStatus = 'Ready' | 'Standby' | 'Out';

const FORM_ID = 'Winlink_Check-In';

/** Compose-side Winlink Check-In form — pre-fills callsign from config,
 *  grid from PositionArbiter, and net/group from FormDraftLibrary slot.
 *
 * Wire-format contract:
 *   onSubmit emits { tactical_call, op_name, group_net, status, comments,
 *                    grid, initials } — the field IDs that WINLINK_CHECK_IN's
 *   template expects (see checkin.rs::FIELDS). Wire keys are lowercase
 *   snake_case per spec §3 wire convention.
 *
 * Draft contract:
 *   onChange emits the same shape as onSubmit (all user-editable fields).
 *   Excludes slot-list state (ephemeral per session) and position fix
 *   metadata (fresh, source).
 *
 * onChange pattern: fired inside input event handlers, NOT in useEffect dep
 *   arrays. Same rationale as PositionFormV2 — onChange reference changes on
 *   every Compose render; dep-array usage → infinite re-render loop.
 *
 * FormDraftLibrary integration:
 *   Saveable fields: op_name, group_net, comments, initials.
 *   NOT saveable: tactical_call (per-operator config, not per-net),
 *   status (volatile per check-in), grid (GPS-derived).
 *   "Save as slot…" always creates a new slot (never updates in place) —
 *   same always-create intent as PositionFormV2 (update-in-place is P3).
 */
export function CheckInForm({
  initialValues,
  onChange,
  onSubmit,
  onCancel,
}: FormComposeProps) {
  const [tacticalCall, setTacticalCall] = useState(initialValues?.tactical_call ?? '');
  const [opName, setOpName] = useState(initialValues?.op_name ?? '');
  const [groupNet, setGroupNet] = useState(initialValues?.group_net ?? '');
  const [status, setStatus] = useState<CheckInStatus>(
    (initialValues?.status as CheckInStatus) ?? 'Ready',
  );
  const [comments, setComments] = useState(initialValues?.comments ?? '');
  const [grid, setGrid] = useState((initialValues?.grid ?? '').toUpperCase());
  const [initials, setInitials] = useState(initialValues?.initials ?? '');

  // FormDraftLibrary slot state.
  const [slots, setSlots] = useState<FormDraftSlot[]>([]);
  const [selectedSlotId, setSelectedSlotId] = useState<string>('');

  // Per-instance suffix for the status radio-group `name`. Without this,
  // two simultaneous CheckInForm mounts (e.g. two Compose windows) would
  // share `name={statusGroupName}` and clicking a radio in one window
  // would deselect the radio in the other (HTML radio groups are scoped
  // by document, not by React tree).
  const statusGroupName = `checkin-status-${useId()}`;

  // Pull callsign from config on mount; only sets if no draft value.
  // Note: does NOT fire onChange (same rationale as PositionFormV2 GPS effect).
  useEffect(() => {
    let mounted = true;
    invoke<Config>('config_read')
      .then((cfg) => {
        if (!mounted) return;
        if (cfg?.callsign && !initialValues?.tactical_call) {
          setTacticalCall(cfg.callsign);
        }
      })
      .catch(() => {/* leave blank — operator fills in */});
    return () => { mounted = false; };
    // initialValues.tactical_call captured at mount; don't re-run on parent re-render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Pull current position fix from PositionArbiter on mount.
  // Only sets grid if no draft value present.
  useEffect(() => {
    let mounted = true;
    invoke<PositionFix>('position_current_fix')
      .then((fix) => {
        if (!mounted) return;
        if (fix.grid && !initialValues?.grid) {
          setGrid(fix.grid.toUpperCase());
        }
      })
      .catch(() => {/* leave blank — operator fills in */});
    return () => { mounted = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Load saved slots on mount. Error → empty list (non-fatal).
  useEffect(() => {
    listSlots(FORM_ID).then(setSlots).catch(() => setSlots([]));
  }, []);

  function buildPayload() {
    return {
      tactical_call: tacticalCall,
      op_name: opName,
      group_net: groupNet,
      status,
      comments,
      grid,
      initials,
    };
  }

  function applySlot(slotId: string) {
    setSelectedSlotId(slotId);
    if (!slotId) return;
    const slot = slots.find((s) => s.slot_id === slotId);
    if (!slot) return;
    // Apply saveable fields from the slot. Volatile fields (tactical_call,
    // status, grid) are intentionally left at their current values.
    const p = slot.payload;
    const newOpName    = typeof p.op_name   === 'string' ? p.op_name   : opName;
    const newGroupNet  = typeof p.group_net === 'string' ? p.group_net : groupNet;
    const newComments  = typeof p.comments  === 'string' ? p.comments  : comments;
    const newInitials  = typeof p.initials  === 'string' ? p.initials  : initials;
    setOpName(newOpName);
    setGroupNet(newGroupNet);
    setComments(newComments);
    setInitials(newInitials);
    // Construct the emitted payload inline (instead of spreading buildPayload()
    // and overriding the slot fields). buildPayload() reads current React
    // state, which is still pre-slot at this point because state setters are
    // async — the spread + override worked here only because we list every
    // slot-saveable field. Inline construction makes the contract explicit and
    // future-proof against adding new slot-saveable fields.
    onChange?.({
      tactical_call: tacticalCall,
      op_name: newOpName,
      group_net: newGroupNet,
      status,
      comments: newComments,
      grid,
      initials: newInitials,
    });
  }

  async function saveSlot() {
    const label = window.prompt('Name this slot (e.g. "Monday Night Net"):');
    if (!label) return;
    // Always-create intent: no slotId passed even when a slot is selected.
    // Update-in-place is a P3 follow-up (same rationale as PositionFormV2).
    const newSlot = await upsertSlot({
      formId: FORM_ID,
      label,
      payload: { op_name: opName, group_net: groupNet, comments, initials },
    });
    setSlots((prev) => [...prev, newSlot]);
    setSelectedSlotId(newSlot.slot_id);
  }

  async function removeSlot() {
    if (!selectedSlotId) return;
    if (!window.confirm('Delete this saved slot?')) return;
    await deleteSlot(selectedSlotId);
    setSlots((prev) => prev.filter((s) => s.slot_id !== selectedSlotId));
    setSelectedSlotId('');
  }

  function handleSend() {
    onSubmit(buildPayload());
  }

  return (
    <div className="checkin-form" data-testid="checkin-form">
      {/* ── Saved slots toolbar ── */}
      <div className="form-slot-toolbar" data-testid="slot-toolbar">
        <label htmlFor="checkin-slot-select">Saved slots:</label>
        <select
          id="checkin-slot-select"
          value={selectedSlotId}
          onChange={(e) => applySlot(e.target.value)}
        >
          <option value="">— None —</option>
          {slots.map((s) => (
            <option key={s.slot_id} value={s.slot_id}>{s.label}</option>
          ))}
        </select>
        <button type="button" onClick={saveSlot} data-testid="slot-save-btn">
          Save as slot…
        </button>
        {selectedSlotId && (
          <button type="button" onClick={removeSlot} data-testid="slot-delete-btn">
            Delete
          </button>
        )}
      </div>

      <div className="checkin-form__header">
        <h2>Winlink Check-In</h2>
      </div>

      {/* ── Tactical Call ── */}
      <label htmlFor="checkin-tactical-call">Tactical Call</label>
      <input
        id="checkin-tactical-call"
        type="text"
        value={tacticalCall}
        onChange={(e) => {
          const v = e.target.value;
          setTacticalCall(v);
          onChange?.({ ...buildPayload(), tactical_call: v });
        }}
        placeholder="W7CPZ"
      />

      {/* ── Operator Name ── */}
      <label htmlFor="checkin-op-name">Operator Name</label>
      <input
        id="checkin-op-name"
        type="text"
        value={opName}
        onChange={(e) => {
          const v = e.target.value;
          setOpName(v);
          onChange?.({ ...buildPayload(), op_name: v });
        }}
        placeholder="John Smith"
      />

      {/* ── Group / Net ── */}
      <label htmlFor="checkin-group-net">Group / Net</label>
      <input
        id="checkin-group-net"
        type="text"
        value={groupNet}
        onChange={(e) => {
          const v = e.target.value;
          setGroupNet(v);
          onChange?.({ ...buildPayload(), group_net: v });
        }}
        placeholder="Cascadia ARES Net"
      />

      {/* ── Status ── */}
      <fieldset className="checkin-form__status-fieldset">
        <legend>Status</legend>
        <label>
          <input
            type="radio"
            name={statusGroupName}
            checked={status === 'Ready'}
            onChange={() => {
              setStatus('Ready');
              onChange?.({ ...buildPayload(), status: 'Ready' });
            }}
          />
          {' '}Ready
        </label>
        <label>
          <input
            type="radio"
            name={statusGroupName}
            checked={status === 'Standby'}
            onChange={() => {
              setStatus('Standby');
              onChange?.({ ...buildPayload(), status: 'Standby' });
            }}
          />
          {' '}Standby
        </label>
        <label>
          <input
            type="radio"
            name={statusGroupName}
            checked={status === 'Out'}
            onChange={() => {
              setStatus('Out');
              onChange?.({ ...buildPayload(), status: 'Out' });
            }}
          />
          {' '}Out
        </label>
      </fieldset>

      {/* ── Grid Square (auto-filled) ── */}
      <label htmlFor="checkin-grid">Grid Square</label>
      <input
        id="checkin-grid"
        type="text"
        value={grid}
        onChange={(e) => {
          const v = e.target.value.toUpperCase();
          setGrid(v);
          onChange?.({ ...buildPayload(), grid: v });
        }}
        placeholder="CN87"
      />

      {/* ── Comments ── */}
      <label htmlFor="checkin-comments">Comments</label>
      <textarea
        id="checkin-comments"
        value={comments}
        rows={3}
        onChange={(e) => {
          const v = e.target.value;
          setComments(v);
          onChange?.({ ...buildPayload(), comments: v });
        }}
      />

      {/* ── Initials ── */}
      <label htmlFor="checkin-initials">Initials</label>
      <input
        id="checkin-initials"
        type="text"
        value={initials}
        onChange={(e) => {
          const v = e.target.value;
          setInitials(v);
          onChange?.({ ...buildPayload(), initials: v });
        }}
        placeholder="JPS"
      />

      <div className="checkin-form__actions">
        <button type="button" onClick={onCancel}>Cancel</button>
        <button
          type="button"
          className="primary"
          onClick={handleSend}
          disabled={!tacticalCall}
        >
          Send
        </button>
      </div>
    </div>
  );
}
