//! Contacts — the address book AND, since the 2026-07-10/11 contacts-superset
//! pivot (spec §AMENDMENT), the roster of observed P2P stations: `Contact`
//! carries a tier (confirmed/unconfirmed) plus reachability
//! (channels/endpoints/grid); the observation recorder + inbound limiter live
//! here (the separate peers store died with the pivot).
pub mod commands;
pub mod limiter;
pub mod observation;
pub mod reachability;
pub mod store;
pub mod suggest;
