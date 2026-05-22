# AX.25 Packet — protocol ground-truth findings (RE synthesis)

> **Status:** research synthesis feeding the v0.1 AX.25 packet brainstorm/spec.
> **Method:** triangulated across three independent authoritative sources, since
> Winlink prose docs are unreliable (see memory `feedback-winlink-re-authoritative-sources`).
> **Date:** 2026-05-22 · **Agent:** sorrel-moss-hemlock.
> Raw evidence (gitignored, local-only): `dev/scratch/winlink-re/findings/0{1,2,3}-*.md`
> and decompiled trees under `dev/scratch/winlink-re/decompiled/`.

## Sources

1. **wl2k-go `v1.0.1`** + **Pat** source (`dev/scratch/tuxlink-pat/`, `~/go/pkg/mod/.../wl2k-go@v1.0.1`).
2. **Winlink channel data + RMS Trimode changelog** (decompiled-adjacent: `Def_RMS_Channels_Ham.zip`, ini files).
3. **Decompiled RMS Express.exe + TNCKissInterface.dll + MD5lib.dll** (the official client, VB.NET → ilspycmd).

## Mode taxonomy (cross-confirmed)

A Winlink "connection" is **(transport × session-role)**:

- **Transports:** Telnet, **Packet (AX.25)**, Pactor, Robust Packet (HF, 300-baud SCS — *not* VHF AX.25), ARDOP, VARA HF, VARA FM, Iridium GO.
- **Session roles** (`B2SessionType`): `CMS` (Winlink/gateway), `P2P`, `RadioOnly`, `PostOffice`, `MESH`, `Automatic`.

For **AX.25 1200-baud packet**, the relevant cells are **Packet+CMS** (dial an RMS Packet gateway) and **Packet+P2P** (dial another station directly).

## Who runs AX.25 connected-mode (the load-bearing finding)

Winlink Packet is **connected-mode AX.25 ARQ** (SABM→UA, sequenced I-frames, RR/RNR/REJ, T1 retransmit, mod-8/mod-128, FCS, ≤2 digipeaters). The state machine has to live *somewhere*, and the two reference implementations make **opposite** choices:

| Implementation | Who does connected-mode AX.25 | Backends |
|---|---|---|
| **RMS Express (official client)** | **The host, in software** — `TNCKissInterface.dll` is a full AX.25 v2.0/v2.2 stack (`Connection`, `DataLinkProvider`, `EstablishDataLink` sends SABM, T1 retransmit). The KISS modem is a dumb HDLC framer. | **KISS over serial COM** and **KISS over TCP** (`COMPort("TCP",host,port)`, e.g. Dire Wolf/soundmodem 127.0.0.1:8100). Also vendor host-mode TNCs (Kantronics/Timewave/SCS) where firmware does it. **No AGWPE.** |
| **wl2k-go / Pat** | **Delegated** — implements *none* of connected-mode itself. | `ax25+linux` → **Linux kernel** AF_AX25 (needs `kissattach`/`libax25`/root); `ax25+serial-tnc` → hardware TNC firmware; `ax25+agwpe` → **AGWPE server** (Dire Wolf/QtSoundModem) does it. **No userspace KISS at all.** |

**Implication for tuxlink:** the operator's framing (Dire Wolf/soundmodem on localhost:port, USB-serial COM KISS, Bluetooth COM KISS — all *direct KISS*) maps exactly onto the **RMS Express model**: tuxlink implements host-side AX.25 connected-mode over KISS, with three interchangeable byte-pipes (TCP / USB-serial / BT-serial). There is **no host-side KISS+AX.25 code to borrow from wl2k-go** — it would be net-new Rust (a KISS framer + an AX.25 v2.x data-link state machine). This is the single largest, most correctness-critical component of the feature.

## P2P vs gateway — session differences (resolves the brainstorm correction)

The operator was right that P2P is a distinct mode with no user auth. Ground truth refines it:

- **Secure-login is conditional, not universal.** A CMS/RMS-gateway peer sends a `;PQ:` challenge; the dialer answers `;PR: <token>`. **P2P peers never challenge** → no auth at all (packet P2P trusts the RF callsign only). `secure_login = MD5(challenge ‖ password ‖ 64-byte-fixed-salt)`, fold first 4 digest bytes → 8 decimal digits. It lives at the **B2F layer, transport-agnostic**. tuxlink's existing `winlink/session.rs` *already* handles "challenge present → respond, absent → skip."
- **FBB master/slave roles:** the **answering/listening** station is *master* (sends MOTD+SID first); the **dialing/initiating** station is *slave* (reads first, then takes the first message turn). tuxlink's existing `run_exchange` is written from the **slave/dialer** perspective ("server speaks first") — which is correct **as long as tuxlink always dials out**.
- **Packet P2P is bidirectional — it both calls AND answers.** (Corrected 2026-05-22 via operator ground truth + code re-verification; an earlier decompile pass wrongly concluded "outbound-only" by tracing only `PacketP2PSession.DoStart→Connect`.) The same `PacketP2PSession` handles being *called*: `blnCalling` defaults `false`, `DoStart` (the dial path) sets it `true`, and the poll loop's `"*** CONNECTED"` handler calls `B2OnConnected(remoteCall, !blnCalling)` — so when not calling, it runs the session **inbound** (`PacketP2PSession.cs:2410-2423`). This is logically required: P2P can't work unless one side answers. The HF P2P sessions show the same shape explicitly (`blnListening` + `"Disconnected/Listening"` + `B2OnConnected(call, blnListening)`, e.g. `ArdopSession.cs:3026`, `PactorWL2KSession.cs:3305`).
- **The answerer is FBB *master*** (speaks first: MOTD+SID), the dialer is *slave*. `B2OnConnected(call, blnInbound)` sets `B2PeerToPeer` and the role.

**Implication for tuxlink:**
- **Dialing** (gateway or peer): tuxlink is slave → the existing slave-role `run_exchange` applies; the only difference between gateway and peer dial is the conditional `;PQ`/`;PR` (already handled).
- **Answering** (P2P listen): tuxlink is master → needs a **master-role B2F path** that does NOT exist yet (`run_exchange` is slave-only: "server speaks first"). Master sends the handshake first and the *remote* (slave) takes the first message turn — both inverted from the current loop. Parameterize the exchange by role (or add `run_exchange_master`).
- The AX.25 link layer needs the **answer path** (bind an inbound SABM → reply UA) plus a **listener lifecycle** (arm channel → wait for inbound CONNECTED → hand to master-role exchange → re-arm). The exact RMS Express listen-arming gesture (auto-answer while the session window is open vs. an explicit toggle) is left to pin down in the spec; what's settled is that answering is in scope.

## Packet config parameters (from RMS Express.ini `[Packet TNC]`)

The AX.25 timing/windowing knobs tuxlink's state machine must expose: `Air Data Rate` 1200/9600, `TX Delay` (TXDELAY), `Max Frame Size` (PACLEN), `Max Frames` (MAXFRAME), `Frack`, `Persistence`, `Slot Time`, `Max Retries` (RETRY); `Packet TNC Model = KISS` with `ACKMODE|NORMAL`; KISS over serial COM **or** TCP (`TCPHost`/`TCPPort`).

## Gateway selection is local-config-first

The default Ham channel download is **HF-only (zero entries ≥144 MHz)** — VHF/UHF packet gateways are **not** distributed; operators enter them locally (callsign + frequency). So tuxlink's packet-gateway UX should be **manual entry of gateway callsign + frequency**, not a directory picker (a directory sync could come later).

## Net design implications for tuxlink v0.1

1. **New transport stack (the big build):** KISS framing + host-side AX.25 v2.x connected-mode state machine, in Rust. → gets the full robustness pipeline (TDD + cross-provider Codex adrev) per the discipline-triage rule (correctness-critical, hard-to-undo).
2. **Three byte-pipe adapters** behind one trait: TCP (Dire Wolf/soundmodem KISS port), USB-serial, Bluetooth-serial (RFCOMM).
3. **Session layer — partial reuse + one new path:** *dialing* (gateway + P2P) reuses the existing slave-role `run_exchange` (difference is only conditional secure-login); *answering* (P2P listen) needs a **new master-role B2F path** (handshake-first + remote-takes-first-turn) — the existing loop is slave-only.
4. **In scope for v0.1:** Packet **dial** (gateway w/ secure-login + peer w/o auth) **and** Packet **answer** (P2P listen); all three KISS byte-pipes (TCP, USB-serial, BT-serial); 1200 baud.
5. **Out of scope for v0.1 (defer):** AGWPE, Linux-kernel-AX.25 path, RadioOnly/PostOffice/MESH roles, 9600 baud, mod-128 (mod-8 first), digipeater-path UI beyond a basic relay field, packet-gateway directory sync.
6. **To verify in the spec** (not yet nailed): exact FBB turn-order for both roles vs the existing `run_exchange` loop; the RMS Express listen-arming gesture; whether any SID bit differs (RE says no P2P-specific SID flag — same `B2FHM$`); KISS ACKMODE handling.
