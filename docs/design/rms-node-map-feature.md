# Feature capture — RMS node map layer: nearest Winlink RMS node by real GPS position (Tuxlink ⇄ Geographica)

**Captured:** 2026-05-22 by `hemlock-arroyo-mink` · **Status:** captured, not designed. Future feature. Tracked by bd issue **`tuxlink-4je`** (depends on `tuxlink-686`, the position subsystem). This doc is the durable, in-repo record of the capture.

**A Tuxlink feature that touches both Tuxlink and Geographica in equal measure** — Tuxlink owns the CMS-side data + the nearest-node logic; Geographica owns the offline-map render. (Tuxlink polls the CMS, not Geographica.)

## Origin

Operator-relayed ask (quoted):

> "Has anyone figured out how to add Winlink RMS nodes as a map layer to either YAAC or Navit? I know that the pat winlink mode will show distance. But to my understanding, the distance calculation is made from the center of the grid square stored in ET-user. This is totally acceptable for HF modes, but is more important for VHF packet or Vara FM. I also have a GPS unit. So it'd be nice to pull up an offline map and see the node that is actually closest to me."

Operator's note: **"We can already do this if we have access to the WL2K RMS node list, which is a thing you can request from the CMS."**

## Value proposition

Pat computes RMS distance from the **center of the user's own grid square** (the `ET-user` entry). With a GPS unit, the operator knows their **precise** position. Using precise GPS as the distance origin — instead of grid-square-center — is:

- **Acceptable to skip for HF** (long range; grid-center error is negligible relative to skip distance).
- **Materially better for VHF packet / VARA FM** (short, near-line-of-sight range; a grid square is ~mi across, so grid-center error can flip which node is genuinely closest/reachable).

Deliverable: pull up an **offline** map and see the RMS node **actually closest to me**, by real GPS position.

## Architecture — who owns what

**Tuxlink owns (the data + logic half):**
- Fetch the **WL2K RMS node list from the CMS** (callsign, frequency, mode, grid square / position, possibly hours/service). Operator confirms this list is requestable from the CMS.
- Compute nearest-node distance from the operator's **precise GPS position** (not grid-center) to each node.
- Leverages the **position subsystem (`tuxlink-686`)** (Maidenhead ↔ lat/lon conversion, distance) — that's the enabling piece, hence the `bd dep`.

**Geographica owns (the render half):**
- Offline base map + render the RMS node set as a **map layer / POI overlay**.
- Highlight the nearest node; show the operator's GPS position.

The cross-project boundary: Tuxlink produces a node dataset (+ the operator's position); Geographica consumes it as a layer. When the Geographica half is built, track it in Geographica's own tracker with a dep back to `tuxlink-4je` (sibling repos run from their own session roots).

## Correctness / posture notes

- **GPS precision:** the project's precision-reduction default (broadcast position → 4-char Maidenhead by default; opt-in to higher precision) governs **what you transmit**. This nearest-node calc is **local-only** — using **full GPS precision** for it is correct and does NOT conflict with that posture. A future implementer must not down-reduce precision for the local distance calc.
- **Node-position precision:** the win comes from precise *user* position. The *nodes'* positions may themselves only be grid squares in the CMS list — acceptable (nodes are fixed; the dominant error source the operator is complaining about is the user-side grid-center origin).

## Open items to verify when picked up

- **Exact CMS request mechanism** for the RMS node list, and the fields it returns (does it carry precise lat/lon, or grid square only?). Verify against Winlink docs — do not assume (ham-radio specifics are a known-unreliable area).
- **Does `tuxlink-pat` already expose RMS-list retrieval?** Upstream Pat has RMS-list functionality; the fork may already provide the fetch, reducing this to parse + compute + hand-off. Verify in the fork.
- **List freshness / caching** for true offline use (cache last-fetched list on disk; refresh when online).
- **Geographica ingestion format** for a custom POI/node layer (GeoJSON? waypoint set?).
- **Prior art (the quoted open question):** has anyone done RMS-node map layers in **YAAC** or **Navit**? Research when picked up — may inform the layer format and UX.
