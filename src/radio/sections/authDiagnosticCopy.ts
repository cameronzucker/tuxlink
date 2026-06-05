import type { FailureMode, TransportFailureKind } from '../../connections/sessionTypes';

export interface BannerCopy {
  headline: string;
  body: string;
}

export function copyFor({
  mode,
  transportKind,
}: {
  mode: FailureMode;
  transportKind: TransportFailureKind | null;
}): BannerCopy {
  switch (mode) {
    case 'network_unreachable':
      return mode1CopyForKind(transportKind);
    case 'client_rejected':
      return {
        headline: "Tuxlink isn't on the Winlink server's allowlist yet.",
        body: 'This is a known limitation — try cms-z (dev) instead, or send the log to help.',
      };
    case 'password_rejected':
      return {
        headline: "Your password wasn't accepted by the Winlink server.",
        body: 'Reset it on winlink.org or re-enter it here.',
      };
    case 'callsign_rejected':
      return {
        headline: "The Winlink server didn't accept your callsign.",
        body: 'The most common cause is account deactivation (e.g., after a license-renewal gap) — verify your account is active on winlink.org.',
      };
    case 'session_dropped_after_auth':
      return {
        headline: 'Login succeeded, then the connection dropped.',
        body: 'Try connecting again — if this keeps happening, your network path may be flaky or the server may be under load.',
      };
    case 'temporary_server_unavailability':
      return {
        headline: 'The Winlink server is temporarily unavailable.',
        body: 'Try again in a few minutes.',
      };
    case 'uncategorized':
      return {
        headline: 'Connection failed.',
        body: 'The Winlink server returned an unrecognised response — see the wire-response details below, or copy the log to share.',
      };
  }
}

function mode1CopyForKind(kind: TransportFailureKind | null): BannerCopy {
  switch (kind) {
    case 'dns':
      return {
        headline: "Couldn't find the Winlink server's address.",
        body: 'Check the hostname spelling.',
      };
    case 'tcp_refused':
      return {
        headline: 'The Winlink server refused the connection.',
        body: 'It may be offline; check the hostname + port.',
      };
    case 'tcp_timeout':
      return {
        headline: "Couldn't reach the Winlink server within the timeout.",
        body: 'Check your internet connection.',
      };
    case 'tls_handshake':
      return {
        headline: "Couldn't negotiate TLS with the Winlink server.",
        body: 'If you picked the TLS transport but the host only listens on plaintext (or vice-versa), switch transports.',
      };
    case null:
    default:
      return {
        headline: "Couldn't reach the Winlink server.",
        body: 'Check your internet connection.',
      };
  }
}
