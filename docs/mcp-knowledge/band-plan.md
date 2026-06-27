# Band plan reference (Winlink-relevant)

A concise frequency reference for Winlink and related digital operation. This
is a **starting reference, not authoritative** — propagation, regional band
plans, and gateway availability change. Operators verify current dial
frequencies against the live channel/catalog data and their national band
plan before transmitting.

All HF dial frequencies below are **USB dial** frequencies in MHz. The actual
on-air data signal sits above the dial frequency within the audio passband.
Gateway frequencies vary by gateway; use a catalog request to get the current
per-gateway list.

## HF data (ARDOP / VARA HF / HF packet)

| Band | Common Winlink HF activity (USB dial, MHz) | Notes |
|---|---|---|
| 80m | 3.578 – 3.600 region | Regional / NVIS at night |
| 60m | Channelized (5 MHz channels, region-dependent) | Check local channel rules |
| 40m | 7.100 region | Workhorse band; NVIS after dark, wider range by day |
| 30m | 10.140 region | Narrow band; data-friendly, no phone |
| 20m | 14.105 region | Long-haul daytime |
| 17m | 18.106 region | Daytime DX |
| 15m | 21.105 region | Daytime, solar-condition dependent |

These are activity centers, not single fixed channels. Individual RMS
gateways publish their own exact dial frequencies and supported bandwidths;
always connect on the gateway's published frequency.

## VHF/UHF (FM packet and VARA FM)

| Use | Frequency | Notes |
|---|---|---|
| APRS (North America) | 144.390 MHz | National APRS calling/working frequency |
| 2m packet / Winlink Packet | Region-dependent | No single national channel; depends on local gateway |
| 70cm packet / Winlink Packet | Region-dependent | Depends on local gateway |
| VARA FM | Same VHF/UHF channels as packet gateways | Higher throughput than 1200-baud packet on the same chain |

VHF/UHF Winlink Packet and VARA FM coverage is highly regional. Some areas
have several gateways; many have none. Use the catalog to find local gateways
by mode and frequency.

## Choosing a band for range

- **Local / line-of-sight (under ~30 mi):** VHF/UHF FM packet or VARA FM.
- **Regional (roughly 0–400 mi, no skip zone):** HF NVIS on 80m (night) or
  40m (day into evening).
- **Long-haul (hundreds to thousands of miles):** higher HF bands (40m–15m)
  by day, lower bands at night, following normal HF propagation.
