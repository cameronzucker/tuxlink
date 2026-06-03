//! Saildocs GRIB-request support (tuxlink-vrpk).
//!
//! GRIB files in Winlink Express are served by the third-party Saildocs
//! email service, NOT the Winlink CMS itself — see
//! `docs/design/2026-06-02-cms-request-protocol-grounding.md` §"GRIB
//! request via Saildocs" for the empirical grounding (WLE's catalog
//! database has zero GRIB entries; the `WL2K_HELP/CUSTOM.GRIB` doc
//! describes the Saildocs flow).
//!
//! Wire format (canonical, from https://saildocs.com/gribinfo):
//!
//! ```text
//! To:      query@saildocs.com
//! Subject: <operator-editable; default "GRIB request">
//! Body:    send gfs:LAT0,LAT1,LON0,LON1|dlat,dlon|VTs|Params
//! ```
//!
//! Saildocs replies with a Private message carrying a GRIB-1 binary
//! attachment that the operator saves locally and opens in an external
//! viewer (zyGrib, OpenCPN, Expedition). WLE does the same — no in-app
//! GRIB rendering for v0.x.

pub mod commands;
pub mod composer;

pub use composer::{
    build_grib_body, compose_grib_message, ForecastTime, GribComposeError, GribDirection, GribMode,
    GribParameter, GribRequest, Latitude, Longitude, SAILDOCS_RECIPIENT,
};
pub use commands::grib_send_request;
