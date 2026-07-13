# Pat Winlink (third-party client — not Tuxlink)

Reference for **Pat**, a different Winlink client. Use this when helping an operator
who is running Pat. It is not documentation of Tuxlink's own behavior. For how Pat
compares to Tuxlink, see the user-guide topic `32-from-express-or-pat`.

Pat is an open-source cross-platform Winlink client (CLI, interactive prompt, and a
web UI). **EmComm Tools Community (ETC) ships Pat stock** alongside Dire Wolf and the
Linux AX.25 stack, pre-configured — so every connect string below applies verbatim on
ETC. ETC configures the axport and TNC for the operator; it does not change Pat's
syntax.

## Connect-URL grammar

```
transport://[host][/digi]/targetcall[?params...]
```

**The single most-confused point:** `host` addresses the **local TNC or modem**. The
**path** is the **RF route**. They are different things.

- The **last** path element is always the **target callsign**.
- Every path element **before** the target is a **digipeater hop**.
- Hops are separated by **`/`**. **Not commas.**

## Connect via a digipeater

```bash
pat connect "ax25:///W4ABC-1/W4XYZ-10"
```

Reach `W4XYZ-10` **via digipeater `W4ABC-1`**.

Pat's own example, verbatim from `pat connect --help`:

```
connect ax25:///LA1B/LA5NTA          Peer-to-peer connection with LA5NTA via LA1B digipeater.
```

### Multiple digipeaters

Chain them with `/`, in the order they will be used:

```bash
pat connect "ax25:///DIGI1/DIGI2/W4XYZ-10"
```

Pat parses the path into a **list** of digipeaters, so more than one hop is supported.
From Pat's transport source (`la5nta/wl2k-go`, `transport/url.go`) — the load-bearing
lines, abridged:

```go
// scheme://(mycall(:password)@)(host)(/digi1/...)/targetcall
Digis []string   // List of digipeaters ("path" between origin and target).

via, target := path.Split(u.Path)
url.Digis = strings.Split(strings.Trim(via, "/"), "/")
```

`Digis` is a list and the separator is `/`. That is the whole basis for multi-hop.

### The triple slash is not a typo

`ax25:///W4XYZ-10` is `ax25://` + an **empty host** + `/W4XYZ-10`. Leaving the host
empty tells Pat to use the AX.25 engine named in its config. That is the usual form.

### Naming the axport explicitly

When the host **is** given for `ax25+linux`, it is the **axport** (as defined in
`/etc/ax25/axports`), not a radio path:

```bash
pat connect "ax25+linux://tmd710/W4ABC-1/W4XYZ-10"
#                        ^^^^^^ axport   ^^^^^^^ digi  ^^^^^^^^^ target
```

Pat's example: `connect ax25+linux://tmd710/LA1B-10` — axport `tmd710`, no digipeater.

## Transports

| Scheme | Use |
|---|---|
| `telnet` | TCP/IP to a Winlink CMS |
| `ardop` | ARDOP TNC |
| `pactor` | SCS PTC modems |
| `varahf` | VARA HF TNC |
| `varafm` | VARA FM TNC |
| `ax25` | AX.25, engine from config (**default**) |
| `ax25+agwpe` | AX.25 via AGWPE / Dire Wolf |
| `ax25+linux` | AX.25 via the Linux kernel stack |
| `ax25+serial-tnc` | AX.25 via a serial TNC |

**Digipeaters are a packet/AX.25 concept.** Pat *rejects* a digipeater path on
`ardop` and `telnet` with `ErrDigisUnsupported`. Do not suggest a digi path on an
ARDOP connection.

## Parameters

| Param | Effect |
|---|---|
| `?freq=` | QSY the radio via rigcontrol before connecting. **`ardop` and `ax25` only.** |
| `?host=` | Override the host part of the path. `ax25:///LA1B?host=ax0` is the same as `ax25://ax0/LA1B`. |
| `?prehook=` | Run an executable middleware before B2F takes over. Useful for packet-node traversal. |

Example: `pat connect "ardop:///LA3F?freq=5350"` — connect to `LA3F` and set the dial
frequency to 5350 kHz.

## Callsign case

Pat upper-cases the path for you, so `ax25:///la5nta` and `ax25:///LA5NTA` are
equivalent. An operator typing lowercase has not made a mistake.

## CLI, interactive, and web

The same connect string works in all three:

```bash
pat connect "ax25:///W4ABC-1/W4XYZ-10"   # one-shot CLI
pat interactive                          # then: connect ax25:///W4ABC-1/W4XYZ-10
pat http                                 # web UI on localhost:8080
```

Other common commands: `pat compose`, `pat read`, `pat position`, `pat rmslist`
(search RMS gateways), `pat templates` (forms).

## Configuration

`~/.config/pat/config.json` — edit directly, or `pat configure` to open it in an
editor. `pat init` runs first-time setup. Rig control is via hamlib/`rigctld`.

## Common failure to check first

If a digipeater connection fails, confirm the **digipeater and the target station are
on the same frequency** — a digi cannot bridge two frequencies. Then confirm the
digipeater actually repeats for you (some are restricted).
