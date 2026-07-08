# Playbook: picking the right audio device for VARA / a digital modem

Use `ardop_list_audio_devices` to read the audio cards, then **apply this method
yourself** to pick the operator's radio interface. Tuxlink deliberately does NOT
rank or auto-pick a card — the right card depends on the operator's specific
hardware, and a hardcoded guess is how the wrong device gets selected. Guide the
operator to *find theirs*.

## What the tool gives you

`ardop_list_audio_devices` returns a `cards` list; per USB card:

- `name` — the card's human label (e.g. `"USB Audio CODEC"`).
- `alsa_name` — the ALSA `plughw:CARD=…` name VARA/ardopcf actually opens.
- `card_index` — the live `card<N>` boot-order index.
- `vid_pid` — the USB vendor:product id (e.g. `0d8c:013a`).
- `bus_path` — the sysfs USB device-node / port path.
- `in_use` — true when another program currently holds the card.

## The disambiguation method

1. **The radio interface is a full-duplex USB sound card, distinct from any
   headset/webcam.** A USB radio adapter presents BOTH a capture and a playback
   endpoint on the *same* card. A headset also does — so name alone is not
   enough; use VID:PID and the operator's knowledge of what's plugged in.
2. **Confirm capture AND playback on the same card.** VARA needs one full-duplex
   card for both directions. Advise the operator to set VARA **Input AND Output
   to the SAME card** (the `alsa_name` of the radio interface) — never split
   input/output across two cards.
3. **VID:PID identifies the adapter family.** C-Media–based radio adapters (the
   DRA-100 class and many DigiRig-style dongles) report vendor `0d8c` — i.e.
   `vid_pid` starting `0d8c:`. This tells a radio adapter apart from a headset at
   a glance, but confirm with the operator; do not assume `0d8c` == their radio.
4. **Two identical-name cards → split them by `bus_path`, then `in_use`.** If two
   cards share a `name` (two of the same dongle, or a dongle plus a look-alike),
   their `bus_path` (USB port path) differs — that is the reliable discriminator.
   `in_use` is the secondary tie-break: the card another program already holds is
   usually not the one to hand the modem.
5. **PTT is usually a SEPARATE device from audio.** The keying line (CAT/RTS
   serial, or a CM108-family HID) is typically a different USB node than the audio
   card. Do not conflate "which card for audio" with "which line for PTT".

## What NOT to do

- Do not pick a card for the operator from a ranking — present the candidates and
  the method, and let them confirm which is their radio.
- Do not ship a specific bench's identity as "the answer." One operator's DRA-100
  is another's DigiRig; the method is portable, the identity is not.
- Do not split VARA input and output across two different cards.
