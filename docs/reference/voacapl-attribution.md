# VOACAP / voacapl — Attribution and License

Tuxlink bundles two components to provide offline HF propagation prediction:

## VOACAP core engine

The VOACAP propagation model was developed at the Institute for Telecommunication
Sciences (ITS), a laboratory within the U.S. National Telecommunications and
Information Administration (NTIA). As a work of the United States Government,
the VOACAP source code is **public domain** under 17 U.S.C. § 105 — no copyright,
no license required.

The engine performs ITU-R P.533 ionospheric predictions using CCIR (or URSI)
ionospheric coefficient sets. The coefficient data distributed inside itshfbc is
likewise derived from ITU-R archives and is not subject to third-party license
restrictions.

## voacapl Linux port

Jim Watson's `voacapl` project (github.com/jawatson/voacapl) is a Linux port
and build wrapper around the VOACAP Fortran source. Watson released it under the
**Creative Commons Zero 1.0 Universal (CC0)** public domain dedication, which
waives all copyright and related rights to the fullest extent permitted by law.

Repository: https://github.com/jawatson/voacapl

## What is NOT bundled

The `dst2csv` and `dst2ascii` utilities in the voacapl repository are distributed
under the GNU General Public License version 3 (GPL-3.0). Tuxlink does **not**
bundle these utilities and has no dependency on them.

## No warranty

Neither NTIA, ITS, Jim Watson, nor the tuxlink project makes any warranty,
express or implied, regarding the accuracy of propagation predictions or their
suitability for any operational purpose. VOACAP predictions are statistical
estimates derived from empirical ionospheric models. Amateur-radio propagation
involves real-world variables (solar conditions, local terrain, equipment
variations, operator skill) that no model fully captures.

**Do not rely on propagation predictions for safety-of-life communications.**
