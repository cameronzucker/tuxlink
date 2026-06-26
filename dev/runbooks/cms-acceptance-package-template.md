# Tuxlink — Winlink CMS client registration evidence

**Client:** Tuxlink · **SID:** `[tuxlink-<VERSION>-B2FHM$]` · **Version tested:** 0.76.1
**Date:** <YYYY-MM-DD> · **Station:** N7CPZ · **Requested action:** register the
`tuxlink` client SID for production CMS forwarding.

## Summary

Tuxlink is a native Winlink client implementing B2F (B2 compressed forwarding, FBB
basic, hierarchical locators, message-id, BID). It connects to the production CMS
over three transports — Telnet, ARDOP (HF), and AX.25 (packet) — presenting the
identical SID `[tuxlink-0.76.1-B2FHM$]`. The production server rejects the
unregistered SID; the dev target `cms-z.winlink.org` completes the exchange. This
package documents both, per transport, to support registration of the SID.

---

## 1. Telnet — authoritative SID + rejection

- **Endpoint:** `server.winlink.org:8773` (TLS)
- **Date/time (UTC):** <fill>

**Production transcript (raw wire — shows server `[WL2K-…]`, our SID line, `;PR:`,
and the rejection):**

```
<paste raw-wire window contents or attach screenshot>
```

**Dev-target contrast (`cms-z.winlink.org` — successful exchange):**

```
<paste raw-wire window contents or attach screenshot>
```

---

## 2. ARDOP (HF) — carriage over RF

- **Gateway / frequency:** <fill> · **Bandwidth:** 500 Hz · **drive_level:** 40
- **Date/time (UTC):** <fill>

**Client session log (FB/FS exchange or handshake-rejection error):**

```
<paste raw-wire window contents or attach screenshot>
```

**ardopcf modem log (connection + ARQ transcript):**

```
<paste ardopcf WebGUI/console log>
```

---

## 3. AX.25 (packet) — carriage over RF

- **Gateway:** <fill> · **Link:** <Dire Wolf TCP / serial KISS / Bluetooth>
- **Date/time (UTC):** <fill>

**Client session log (FB/FS exchange or handshake-rejection error):**

```
<paste raw-wire window contents or attach screenshot>
```

**Dire Wolf / TNC log (AX.25 frame transcript):**

```
<paste TNC log>
```

---

## Notes for the reviewer

- The SID is constructed once in the client and is identical across all three
  transports; the carriage transport does not alter the B2F identity.
- The production rejection is the expected pre-registration behavior; the dev-target
  success in §1 demonstrates the protocol implementation is correct.
