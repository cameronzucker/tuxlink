#!/usr/bin/env python3
"""Close-serial CAT-PTT bridge for radios that key by CAT command only.

A radio such as the Yaesu FT-710 keys ONLY by CAT command (TX1; / TX0;) on its
serial port; serial RTS/DTR PTT is a no-op. On a single-cable USB tree (hub ->
CP2105 serial + C-Media codec) the codec resets if the serial port is held OPEN
concurrent with audio streaming. The fix proven on air 2026-06-23: key by CAT,
then CLOSE the serial port during audio (the radio stays CAT-latched in TX),
reopening only momentarily to relay each keystring.

ardopcf is launched with `-c TCP:<port> -k <hex(key)> -u <hex(unkey)>` so it
sends its keystring over this TCP socket instead of toggling a serial line. This
bridge listens on that socket, and for each keystring opens the serial port,
writes the bytes, flushes, and closes — momentary open/write/close — so the port
is shut while the codec streams audio.

Parameters are read from argv (see `--help`) so tuxlink can drive any radio in
this class without editing the script. The defaults reproduce the FT-710 setup.

On every connection teardown the bridge sends the unkey command as a failsafe so
a dropped ardopcf socket cannot leave the radio latched in TX.
"""

import argparse
import signal
import socket
import sys
import time

try:
    import serial  # pyserial
except ImportError:
    sys.stderr.write(
        "[bridge] FATAL: pyserial not installed (python3 -m pip install pyserial)\n"
    )
    sys.exit(2)


def parse_args(argv):
    p = argparse.ArgumentParser(description="Close-serial CAT-PTT bridge.")
    p.add_argument("--port", type=int, default=4532,
                   help="TCP port to listen on (loopback). Default 4532.")
    p.add_argument("--serial", default="/dev/ttyUSB0",
                   help="Serial device for CAT. Default /dev/ttyUSB0.")
    p.add_argument("--baud", type=int, default=38400,
                   help="Serial baud rate. Default 38400.")
    p.add_argument("--key", default="TX1;",
                   help="CAT key command. Default TX1;.")
    p.add_argument("--unkey", default="TX0;",
                   help="CAT unkey command. Default TX0;.")
    return p.parse_args(argv)


def main(argv):
    args = parse_args(argv)
    key_bytes = args.key.encode("ascii")
    unkey_bytes = args.unkey.encode("ascii")
    maxlen = max(len(key_bytes), len(unkey_bytes))

    def cat_send(payload, label):
        """Momentary open/write/close so the serial port is shut during audio."""
        try:
            ser = serial.Serial(args.serial, args.baud, timeout=0.4,
                                rtscts=False, dsrdtr=False)
            ser.rts = False
            ser.dtr = False
            ser.write(payload)
            ser.flush()
            time.sleep(0.07)
            ser.close()
            print(f"[bridge] >>> {label}", flush=True)
        except Exception as exc:  # noqa: BLE001 — relay best-effort, never crash
            print(f"[bridge] CAT err {exc!r}", flush=True)

    # Failsafe on termination: tuxlink stops the bridge with SIGINT (escalating
    # to SIGKILL). Send the unkey command on SIGINT/SIGTERM so a teardown while
    # the radio is latched in TX cannot leave it keyed. SIGKILL cannot be caught,
    # but tuxlink only escalates to it after a grace period in which this handler
    # has already run.
    def _failsafe(_signum, _frame):
        cat_send(unkey_bytes, "UNKEY (signal failsafe)")
        sys.exit(0)

    signal.signal(signal.SIGINT, _failsafe)
    signal.signal(signal.SIGTERM, _failsafe)

    srv = socket.socket()
    srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    srv.bind(("127.0.0.1", args.port))
    srv.listen(1)
    print(f"[bridge] listening 127.0.0.1:{args.port} -> {args.serial}@{args.baud} "
          f"key={args.key!r} unkey={args.unkey!r}", flush=True)

    while True:
        conn, _ = srv.accept()
        print("[bridge] ardopcf connected", flush=True)
        buf = b""
        try:
            while True:
                data = conn.recv(256)
                if not data:
                    break
                buf += data
                print(f"[bridge rx] {data!r}", flush=True)
                # Match the FULL configured key/unkey byte sequences (e.g. b"TX1;"
                # / b"TX0;"), not a ';'-delimited substring. This processes
                # commands that carry no ';' terminator, and avoids
                # overlapping-prefix misrouting (e.g. key "TX" vs unkey "TX0"). On
                # any ambiguity the UNKEY wins, so the radio is never accidentally
                # left keyed.
                while True:
                    ki = buf.find(key_bytes)
                    ui = buf.find(unkey_bytes)
                    if ki < 0 and ui < 0:
                        break
                    if ui >= 0 and (ki < 0 or ui <= ki):
                        cat_send(unkey_bytes, "UNKEY")
                        buf = buf[ui + len(unkey_bytes):]
                    else:
                        cat_send(key_bytes, "KEY")
                        buf = buf[ki + len(key_bytes):]
                # No complete command remains. Keep only a tail that could be the
                # prefix of a future full match so unrecognized noise can't grow
                # the buffer without bound.
                if len(buf) >= maxlen:
                    buf = buf[-(maxlen - 1):] if maxlen > 1 else b""
        except Exception as exc:  # noqa: BLE001
            print(f"[bridge] conn end {exc!r}", flush=True)
        finally:
            # Failsafe: never leave the radio latched in TX on a dropped socket.
            cat_send(unkey_bytes, "UNKEY (failsafe)")
            conn.close()


if __name__ == "__main__":
    main(sys.argv[1:])
