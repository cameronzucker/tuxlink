# Vendored: wine-vara-setup

This directory is a vendored copy of the engine from
https://github.com/cameronzucker/wine-vara-setup (MIT), bundled into Tuxlink's
package so VARA HF provisioning logic is present offline.

Upstream commit: 5c43d51cc263f5d565468f700be7dc92a8967d6d

Do not edit here — change upstream and re-vendor. Only bin/ and lib/ are needed
at runtime; Tuxlink invokes `bash bin/wine-vara-setup ...`.
