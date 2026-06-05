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

/** Subset of ConfigViewDto returned by `config_read` (only the fields this
 *  form pre-fills from). The wider DTO has connect/transport/host/etc. */
interface ConfigDto {
  callsign?: string;
  identifier?: string;
  grid?: string;
}

type CheckInStatus = 'EXERCISE' | 'REAL EVENT';
type CheckInService = 'AMATEUR' | 'SHARES';
type CheckInBand = 'NA' | 'Telnet' | 'HF' | 'VHF' | 'UHF' | 'SHF';
type CheckInSession =
  | 'Telnet'
  | 'Packet'
  | 'Pactor'
  | 'Robust Packet'
  | 'Ardop'
  | 'VARA HF'
  | 'VARA FM'
  | 'Iridium Go'
  | 'Mesh';

const FORM_ID = 'Winlink_Check-In';
// Exact string from the bundled WLE Winlink_Check_In_Initial.html
// hidden field; rendered as a body line by the Viewer template.
const TEMPLATE_VERSION = 'Winlink Check-in 5.1.3';
const MAP_FILENAME = 'Winlink Check-in V5';
const STATUS_OPTIONS: CheckInStatus[] = ['EXERCISE', 'REAL EVENT'];
const SERVICE_OPTIONS: CheckInService[] = ['AMATEUR', 'SHARES'];
const BAND_OPTIONS: CheckInBand[] = ['NA', 'Telnet', 'HF', 'VHF', 'UHF', 'SHF'];
const SESSION_OPTIONS: CheckInSession[] = [
  'Telnet', 'Packet', 'Pactor', 'Robust Packet', 'Ardop',
  'VARA HF', 'VARA FM', 'Iridium Go', 'Mesh',
];

/** Slot-saveable field IDs — operator content that's reused across check-ins
 *  for the same net. Volatile per-checkin fields (subject, exercise_id,
 *  datetime, msgsender, location/grid, comments) are intentionally excluded
 *  so a slot pre-fills the "net metadata" without overwriting current
 *  position or per-event details. */
/* Slot-saveable fields (the "which net is this" metadata): organization,
 * msgto, contactname, assigned, status, service, band, session. Volatile
 * fields (newsubject, exercise_id, datetime, msgsender, location*, comments)
 * are intentionally excluded — a slot pre-fills the net identity without
 * overwriting per-checkin state or GPS-derived location. applySlot type-
 * checks each field inline; saveSlot enumerates them explicitly when
 * building the slot payload. */

function currentDatetimeIsoMinute(): string {
  // WLE's DateTime field is operator-facing UTC; format YYYY-MM-DD HH:MM
  // matches the prompt default in Winlink_Check_In_Initial.html.
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ` +
    `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
}

/** Compose-side Winlink Check-In form. Field schema fully aligned with WLE
 *  `Winlink_Check_In_Initial.html` so the message renders correctly in the
 *  receive-side WLE viewer + is recognized by CMS / other Winlink clients
 *  as a standard Check-In (not a generic message).
 *
 * Wire-format contract:
 *   onSubmit emits all field IDs from checkin.rs::FIELDS — exhaustively
 *   verified by the wire-format-alignment test below. Per spec §3, wire
 *   keys are lowercase snake_case; the WLE viewer's case-insensitive
 *   `<var>` substitution renders the CamelCase placeholders.
 *
 * Auto-fill sources:
 *   - msgsender ← config_read.callsign (operator's amateur call)
 *   - contactname ← config_read.identifier ?? '' (operator's full name)
 *   - grid ← position_current_fix.grid (PositionArbiter)
 *   - datetime ← UTC now (formatted YYYY-MM-DD HH:MM)
 *   - locationsource ← "GPS" when PositionArbiter has a fresh grid, else "Operator"
 *   - templateversion + mapfilename ← static template metadata
 *
 * Defaults match WLE template defaults: organization="Winlink Net",
 * status="EXERCISE" (intentional — the safe default; real-event check-ins
 * are an explicit operator action), service="AMATEUR", band="NA",
 * session="Telnet".
 *
 * onChange pattern: fired inside input event handlers, never in useEffect
 *   dep arrays (project convention; PositionFormV2 commit c1b122f for the
 *   canonical fix and the loop class it prevents).
 *
 * FormDraftLibrary integration:
 *   Saveable: organization, msgto, contactname, assigned, status, service,
 *   band, session — the "which net is this" metadata.
 *   NOT saveable: newsubject, exercise_id, datetime, msgsender (config),
 *   location/maplat/maplon/mgrs/grid/locationsource (GPS-derived), comments.
 *   "Save as slot…" always creates a new slot (never updates in place) —
 *   project-wide always-create intent per PositionFormV2 precedent. */
export function CheckInForm({
  initialValues,
  onChange,
  onSubmit,
  onCancel,
}: FormComposeProps) {
  // 0. HEADER
  const [organization, setOrganization] = useState(initialValues?.organization ?? 'Winlink Net');
  const [newsubject, setNewsubject] = useState(initialValues?.newsubject ?? '');
  const [exerciseId, setExerciseId] = useState(initialValues?.exercise_id ?? '');

  // 1. STATION
  const [datetime, setDatetime] = useState(initialValues?.datetime ?? currentDatetimeIsoMinute());
  const [msgto, setMsgto] = useState(initialValues?.msgto ?? '');
  const [msgsender, setMsgsender] = useState(initialValues?.msgsender ?? '');
  const [contactname, setContactname] = useState(initialValues?.contactname ?? '');
  const [assigned, setAssigned] = useState(initialValues?.assigned ?? '');

  // 2. SESSION
  const [status, setStatus] = useState<CheckInStatus>(
    (initialValues?.status as CheckInStatus) ?? 'EXERCISE',
  );
  const [service, setService] = useState<CheckInService>(
    (initialValues?.service as CheckInService) ?? 'AMATEUR',
  );
  const [band, setBand] = useState<CheckInBand>(
    (initialValues?.band as CheckInBand) ?? 'NA',
  );
  const [session, setSession] = useState<CheckInSession>(
    (initialValues?.session as CheckInSession) ?? 'Telnet',
  );

  // 3. LOCATION
  const [location, setLocation] = useState(initialValues?.location ?? '');
  const [maplat, setMaplat] = useState(initialValues?.maplat ?? '');
  const [maplon, setMaplon] = useState(initialValues?.maplon ?? '');
  const [mgrs, setMgrs] = useState(initialValues?.mgrs ?? '');
  const [grid, setGrid] = useState((initialValues?.grid ?? '').toUpperCase());
  const [locationsource, setLocationsource] = useState(initialValues?.locationsource ?? 'Operator');

  // 4. COMMENTS
  const [comments, setComments] = useState(initialValues?.comments ?? '');

  // FormDraftLibrary slot state.
  const [slots, setSlots] = useState<FormDraftSlot[]>([]);
  const [selectedSlotId, setSelectedSlotId] = useState<string>('');

  // Per-instance suffixes for radio-group `name`s. Without these, two
  // simultaneous CheckInForm mounts (e.g. two Compose windows) would share
  // document-scoped radio-group names → clicking a radio in one window would
  // deselect the radio in the other.
  const uid = useId();
  const statusGroupName  = `checkin-status-${uid}`;
  const serviceGroupName = `checkin-service-${uid}`;
  const bandGroupName    = `checkin-band-${uid}`;
  const sessionGroupName = `checkin-session-${uid}`;

  // Pull callsign + identifier from config on mount; only set if no draft
  // value present. Does NOT fire onChange (same rationale as the GPS effect
  // in PositionFormV2 — async-arrived defaults shouldn't write through
  // draft state). Operator can still edit either field manually.
  useEffect(() => {
    let mounted = true;
    invoke<ConfigDto>('config_read')
      .then((cfg) => {
        if (!mounted) return;
        if (cfg?.callsign && !initialValues?.msgsender) {
          setMsgsender(cfg.callsign);
        }
        if (cfg?.identifier && !initialValues?.contactname) {
          setContactname(cfg.identifier);
        }
      })
      .catch(() => {/* leave blank — operator fills in */});
    return () => { mounted = false; };
    // initialValues captured at mount; don't re-run on parent re-render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Pull current position fix from PositionArbiter on mount. Sets grid (if
  // operator hadn't pre-filled via draft) and bumps locationsource to "GPS"
  // when the fix is fresh — matches WLE locationSource semantics.
  useEffect(() => {
    let mounted = true;
    invoke<PositionFix>('position_current_fix')
      .then((fix) => {
        if (!mounted) return;
        if (fix.grid && !initialValues?.grid) {
          setGrid(fix.grid.toUpperCase());
        }
        if (fix.fresh && fix.grid && !initialValues?.locationsource) {
          setLocationsource('GPS');
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

  function buildPayload(): Record<string, string> {
    return {
      organization,
      newsubject,
      exercise_id: exerciseId,
      datetime,
      msgto,
      msgsender,
      contactname,
      assigned,
      status,
      service,
      band,
      session,
      location,
      maplat,
      maplon,
      mgrs,
      grid,
      locationsource,
      comments,
      templateversion: TEMPLATE_VERSION,
      mapfilename: MAP_FILENAME,
    };
  }

  function applySlot(slotId: string) {
    setSelectedSlotId(slotId);
    if (!slotId) return;
    const slot = slots.find((s) => s.slot_id === slotId);
    if (!slot) return;
    // Apply only saveable fields. The volatile fields stay at their current
    // values (datetime auto-now, position fields from GPS, etc.).
    const p = slot.payload;

    // Track what actually got applied so the onChange spread doesn't leak
    // unvalidated radio values into draft autosave. Codex 2026-06-05 P2 #5:
    // an old simplified-schema slot with `status: "Ready"` would have its
    // state-setter call rejected by the type-narrowing guard, BUT the prior
    // implementation still spread `newVals.status` into onChange — Compose
    // would persist the invalid value into the draft. Fix: build `applied`
    // alongside the setter calls, then spread only that.
    const applied: Record<string, string> = {};

    if (typeof p.organization === 'string') {
      setOrganization(p.organization);
      applied.organization = p.organization;
    }
    if (typeof p.msgto === 'string') {
      setMsgto(p.msgto);
      applied.msgto = p.msgto;
    }
    if (typeof p.contactname === 'string') {
      setContactname(p.contactname);
      applied.contactname = p.contactname;
    }
    if (typeof p.assigned === 'string') {
      setAssigned(p.assigned);
      applied.assigned = p.assigned;
    }
    if (typeof p.status === 'string' && (STATUS_OPTIONS as string[]).includes(p.status)) {
      setStatus(p.status as CheckInStatus);
      applied.status = p.status;
    }
    if (typeof p.service === 'string' && (SERVICE_OPTIONS as string[]).includes(p.service)) {
      setService(p.service as CheckInService);
      applied.service = p.service;
    }
    if (typeof p.band === 'string' && (BAND_OPTIONS as string[]).includes(p.band)) {
      setBand(p.band as CheckInBand);
      applied.band = p.band;
    }
    if (typeof p.session === 'string' && (SESSION_OPTIONS as string[]).includes(p.session)) {
      setSession(p.session as CheckInSession);
      applied.session = p.session;
    }

    // Construct the emitted payload inline. State setters are async, so
    // buildPayload() here would read pre-slot values. Overlay only the
    // validated `applied` keys on the current state.
    onChange?.({ ...buildPayload(), ...applied });
  }

  async function saveSlot() {
    const label = window.prompt('Name this slot (e.g. "Cascadia ARES Net"):');
    if (!label?.trim()) return;
    // Always-create intent: no slotId passed even when a slot is selected.
    // Update-in-place is a P3 follow-up (same rationale as PositionFormV2).
    const payload: Record<string, string> = {
      organization, msgto, contactname, assigned,
      status, service, band, session,
    };
    const newSlot = await upsertSlot({
      formId: FORM_ID,
      label: label.trim(),
      payload,
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
    // Refresh datetime to the moment of send (operator may have had the
    // form open for minutes; the wire-format DateTime should reflect when
    // the message was actually sent, not when the form was mounted).
    const finalPayload = { ...buildPayload(), datetime: currentDatetimeIsoMinute() };
    onSubmit(finalPayload);
  }

  // Send is gated on the WLE-required fields: organization, newsubject,
  // datetime (auto), msgto, msgsender, contactname, location. The WLE
  // authoring HTML marks Location as `required="required"`; matching that
  // here so the native form's send-gate is no looser than the webview form.
  const canSubmit =
    organization.trim().length > 0 &&
    newsubject.trim().length > 0 &&
    msgto.trim().length > 0 &&
    msgsender.trim().length > 0 &&
    contactname.trim().length > 0 &&
    location.trim().length > 0;

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

      {/* ── 0. HEADER ── */}
      <fieldset className="checkin-form__section">
        <legend>Net</legend>

        <label htmlFor="checkin-organization">Organization</label>
        <input
          id="checkin-organization"
          type="text"
          value={organization}
          maxLength={60}
          onChange={(e) => {
            const v = e.target.value;
            setOrganization(v);
            onChange?.({ ...buildPayload(), organization: v });
          }}
          placeholder="Cascadia ARES Net"
        />

        <label htmlFor="checkin-newsubject">Subject</label>
        <input
          id="checkin-newsubject"
          type="text"
          value={newsubject}
          maxLength={80}
          onChange={(e) => {
            const v = e.target.value;
            setNewsubject(v);
            onChange?.({ ...buildPayload(), newsubject: v });
          }}
          placeholder="Weekly check-in"
        />

        <label htmlFor="checkin-exercise-id">Event / Exercise ID <span className="checkin-form__optional">(optional)</span></label>
        <input
          id="checkin-exercise-id"
          type="text"
          value={exerciseId}
          maxLength={25}
          onChange={(e) => {
            const v = e.target.value.toUpperCase();
            setExerciseId(v);
            onChange?.({ ...buildPayload(), exercise_id: v });
          }}
          placeholder="SHAKEOUT-2026"
        />
      </fieldset>

      {/* ── 1. STATION ── */}
      <fieldset className="checkin-form__section">
        <legend>Station</legend>

        <label htmlFor="checkin-datetime">Date/Time (UTC)</label>
        <input
          id="checkin-datetime"
          type="text"
          value={datetime}
          maxLength={30}
          onChange={(e) => {
            const v = e.target.value;
            setDatetime(v);
            onChange?.({ ...buildPayload(), datetime: v });
          }}
          placeholder="YYYY-MM-DD HH:MM"
        />

        <label htmlFor="checkin-msgto">To</label>
        <input
          id="checkin-msgto"
          type="text"
          value={msgto}
          maxLength={75}
          onChange={(e) => {
            const v = e.target.value;
            setMsgto(v);
            onChange?.({ ...buildPayload(), msgto: v });
          }}
          placeholder="WL-NET"
        />

        <label htmlFor="checkin-msgsender">From (Callsign)</label>
        <input
          id="checkin-msgsender"
          type="text"
          value={msgsender}
          maxLength={12}
          onChange={(e) => {
            const v = e.target.value.toUpperCase();
            setMsgsender(v);
            onChange?.({ ...buildPayload(), msgsender: v });
          }}
          placeholder="W7CPZ"
        />

        <label htmlFor="checkin-contactname">Station Contact Name</label>
        <input
          id="checkin-contactname"
          type="text"
          value={contactname}
          maxLength={60}
          onChange={(e) => {
            const v = e.target.value;
            setContactname(v);
            onChange?.({ ...buildPayload(), contactname: v });
          }}
          placeholder="John Smith"
        />

        <label htmlFor="checkin-assigned">Initial Operators <span className="checkin-form__optional">(optional)</span></label>
        <input
          id="checkin-assigned"
          type="text"
          value={assigned}
          maxLength={60}
          onChange={(e) => {
            const v = e.target.value;
            setAssigned(v);
            onChange?.({ ...buildPayload(), assigned: v });
          }}
          placeholder="W7CPZ, K7XYZ"
        />
      </fieldset>

      {/* ── 2. SESSION ── */}
      <fieldset className="checkin-form__section">
        <legend>Session</legend>

        <fieldset className="checkin-form__radios">
          <legend>Type</legend>
          {STATUS_OPTIONS.map((opt) => (
            <label key={opt}>
              <input
                type="radio"
                name={statusGroupName}
                checked={status === opt}
                onChange={() => {
                  setStatus(opt);
                  onChange?.({ ...buildPayload(), status: opt });
                }}
              />{' '}{opt}
            </label>
          ))}
        </fieldset>

        <fieldset className="checkin-form__radios">
          <legend>Service</legend>
          {SERVICE_OPTIONS.map((opt) => (
            <label key={opt}>
              <input
                type="radio"
                name={serviceGroupName}
                checked={service === opt}
                onChange={() => {
                  setService(opt);
                  onChange?.({ ...buildPayload(), service: opt });
                }}
              />{' '}{opt}
            </label>
          ))}
        </fieldset>

        <fieldset className="checkin-form__radios">
          <legend>Band</legend>
          {BAND_OPTIONS.map((opt) => (
            <label key={opt}>
              <input
                type="radio"
                name={bandGroupName}
                checked={band === opt}
                onChange={() => {
                  setBand(opt);
                  onChange?.({ ...buildPayload(), band: opt });
                }}
              />{' '}{opt}
            </label>
          ))}
        </fieldset>

        <fieldset className="checkin-form__radios">
          <legend>Session Mode</legend>
          {SESSION_OPTIONS.map((opt) => (
            <label key={opt}>
              <input
                type="radio"
                name={sessionGroupName}
                checked={session === opt}
                onChange={() => {
                  setSession(opt);
                  onChange?.({ ...buildPayload(), session: opt });
                }}
              />{' '}{opt}
            </label>
          ))}
        </fieldset>
      </fieldset>

      {/* ── 3. LOCATION ── */}
      <fieldset className="checkin-form__section">
        <legend>Location</legend>

        <label htmlFor="checkin-location">Location description <span className="checkin-form__optional">(optional)</span></label>
        <input
          id="checkin-location"
          type="text"
          value={location}
          maxLength={60}
          onChange={(e) => {
            const v = e.target.value;
            setLocation(v);
            onChange?.({ ...buildPayload(), location: v });
          }}
          placeholder="Home QTH / Field EOC"
        />

        <div className="checkin-form__location-row">
          <div>
            <label htmlFor="checkin-grid">Grid Square</label>
            <input
              id="checkin-grid"
              type="text"
              value={grid}
              maxLength={8}
              onChange={(e) => {
                const v = e.target.value.toUpperCase();
                setGrid(v);
                onChange?.({ ...buildPayload(), grid: v });
              }}
              placeholder="CN87"
            />
          </div>
          <div>
            <label htmlFor="checkin-maplat">Latitude</label>
            <input
              id="checkin-maplat"
              type="text"
              value={maplat}
              maxLength={15}
              onChange={(e) => {
                const v = e.target.value;
                setMaplat(v);
                onChange?.({ ...buildPayload(), maplat: v });
              }}
              placeholder="47.610"
            />
          </div>
          <div>
            <label htmlFor="checkin-maplon">Longitude</label>
            <input
              id="checkin-maplon"
              type="text"
              value={maplon}
              maxLength={15}
              onChange={(e) => {
                const v = e.target.value;
                setMaplon(v);
                onChange?.({ ...buildPayload(), maplon: v });
              }}
              placeholder="-122.330"
            />
          </div>
          <div>
            <label htmlFor="checkin-mgrs">MGRS</label>
            <input
              id="checkin-mgrs"
              type="text"
              value={mgrs}
              maxLength={20}
              onChange={(e) => {
                const v = e.target.value.toUpperCase();
                setMgrs(v);
                onChange?.({ ...buildPayload(), mgrs: v });
              }}
              placeholder="10TET..."
            />
          </div>
        </div>

        <label htmlFor="checkin-locationsource">Location Source</label>
        <input
          id="checkin-locationsource"
          type="text"
          value={locationsource}
          maxLength={20}
          onChange={(e) => {
            const v = e.target.value;
            setLocationsource(v);
            onChange?.({ ...buildPayload(), locationsource: v });
          }}
          placeholder="GPS / Operator / Map"
        />
      </fieldset>

      {/* ── 4. COMMENTS ── */}
      <fieldset className="checkin-form__section">
        <legend>Comments</legend>
        <textarea
          id="checkin-comments"
          value={comments}
          rows={4}
          maxLength={2000}
          onChange={(e) => {
            const v = e.target.value;
            setComments(v);
            onChange?.({ ...buildPayload(), comments: v });
          }}
        />
      </fieldset>

      <div className="checkin-form__actions">
        <button type="button" onClick={onCancel}>Cancel</button>
        <button
          type="button"
          className="primary"
          onClick={handleSend}
          disabled={!canSubmit}
          data-testid="checkin-send-btn"
        >
          Send
        </button>
      </div>
    </div>
  );
}
