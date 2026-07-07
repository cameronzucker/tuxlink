# Vendored: wine-vara-setup

This directory is a vendored copy of the engine from
https://github.com/cameronzucker/wine-vara-setup (MIT), bundled into Tuxlink's
package so VARA HF provisioning logic is present offline.

Upstream commit: 9211cc7ec438085406277fe00a8b9b9659903abe

Do not edit here — change upstream and re-vendor. Only bin/ and lib/ are needed
at runtime; Tuxlink invokes `bash bin/wine-vara-setup ...`.
