# UV-Pro / Benshi control protocol — attribution and license review

Tuxlink's native UV-Pro device-control backend (the `uvpro_*` commands;
`src-tauri/src/winlink/ax25/uvpro/`) speaks the **Benshi/Vero** Bluetooth control
protocol used by the BTECH UV-Pro and related radios. That protocol is
undocumented by the manufacturer. Tuxlink's implementation was derived from two
open-source prior-art projects that decoded it. This page credits them and
records tuxlink's license-compliance reasoning.

## The prior-art projects

### benlink — the protocol decode

**[benlink](https://github.com/khusmann/benlink)** by **Kyle Husmann (KC3SLD)**
is a cross-platform Python library whose stated goal is *to document the Benshi
BLE / RFCOMM protocol* for the UV-Pro, RadioOddity GA-5WB, and Vero VR-N76 /
VR-N7500 family. Its typed bitfield definitions are, in effect, a
machine-readable specification of the wire protocol. tuxlink derived the GAIA
framing, the message header layout, the BasicCommand numbers, and the RfCh /
Status / Settings field layouts primarily from benlink.

- License: **Apache License 2.0**
- Copyright: © Kyle Husmann

### HTCommander — the reference client

**[HTCommander](https://github.com/Ylianst/HTCommander)** by **Ylian
Saint-Hilaire** is a full radio-control client for the same radio family, built
on benlink's decoding work. tuxlink used it to cross-validate the command set,
the connect/hydrate sequence, and the channel-selection mechanism (active channel
via `Settings.channel_a`/`channel_b`, written with `WRITE_SETTINGS`).

- License: **Apache License 2.0**
- Copyright: © Ylian Saint-Hilaire

Two independently-authored implementations that agree on the protocol are the
reverse-engineering equivalent of a passing test; reading both is why tuxlink
treats the derived protocol as trustworthy.

## What tuxlink took, and what it did not

Tuxlink derived the **protocol** — the wire formats, command numbers, framing,
and field layouts — by reading the two implementations above. Tuxlink's
`uvpro/` module is an **independent reimplementation in Rust**: no benlink or
HTCommander source code (Python, C#, comments, file structure, or other creative
expression) was copied into tuxlink.

The codec's test fixtures (`docs/design/uvpro-benshi-golden-vectors.md`) are
example byte sequences obtained by *running benlink's encoder* to emit the
protocol's representation of known inputs. They are factual protocol data, not
copied source code.

## License-compliance reasoning

> This is an engineering compliance analysis, not legal advice. The project owner
> (the tuxlink licensee) makes the final call; this records the reasoning so it is
> auditable.

1. **A communication protocol is functional, not copyrightable expression.** The
   wire formats, command numbers, framing, and field layouts of the Benshi
   protocol are methods of operation / facts. benlink's and HTCommander's
   *source code* is copyrighted; the *protocol they document* is not. tuxlink
   reproduced the protocol (uncopyrightable) and none of their source expression.

2. **tuxlink's `uvpro/` module is therefore an independent work, not a
   "Derivative Work"** of either project under copyright. Apache-2.0's
   redistribution conditions (§4: retain notices, include the license text,
   carry forward a NOTICE file, state changes) attach to "reproduc[ing] or
   distribut[ing] ... the Work or Derivative Works thereof." tuxlink distributes
   neither benlink/HTCommander code nor a derivative of it, so §4 is not
   triggered. (For completeness: neither upstream project ships a `NOTICE` file,
   so there is no NOTICE content to propagate even under the conservative view.)

3. **The golden vectors are factual data.** Running a program to produce a factual
   representation of a protocol does not make the output a derivative work of the
   program. They are safe to commit and distribute.

4. **No license conflict exists even under the most conservative reading.**
   Tuxlink is licensed **GPL-3.0-or-later**. **Apache-2.0 is one-way compatible
   with GPLv3** (per the Free Software Foundation): Apache-2.0-licensed material
   may be incorporated into a GPLv3 project, with the combined work distributed
   under GPLv3. So even if one treated any Apache-2.0 expression as having been
   incorporated (it was not), the result would still comply. The incompatible
   direction — GPL code into an Apache-2.0 project — is not tuxlink's situation.

5. **Attribution regardless.** Crediting the projects and authors that decoded an
   undocumented protocol is the right thing to do independent of strict legal
   obligation. This page, the [credits page](../user-guide/31-credits.md), and the
   design spec are tuxlink's durable attribution. The same posture tuxlink already
   applies to Pat / wl2k-go for the Winlink B2F protocol.

## Thanks

Sincere thanks to **Kyle Husmann (KC3SLD)** and the benlink contributors, and to
**Ylian Saint-Hilaire** and the HTCommander contributors, for the open
reverse-engineering work that made tuxlink's native UV-Pro control possible.

## Reporting an attribution or license issue

If anything here is incorrect, incomplete, or you believe tuxlink's use exceeds
what the analysis above describes, please open an issue on the
[tuxlink repo](https://github.com/cameronzucker/tuxlink). This is living
documentation.
