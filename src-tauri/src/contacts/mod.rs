//! Contacts — the address book AND, since the 2026-07-10/11 contacts-superset
//! pivot (spec §AMENDMENT), the roster of observed P2P stations: `Contact`
//! carries a tier (confirmed/unconfirmed) plus reachability
//! (channels/endpoints/grid).
pub mod commands;
pub mod reachability;
pub mod store;
pub mod suggest;
