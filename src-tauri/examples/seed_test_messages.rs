//! Seed the native mailbox with synthetic test messages for find-messages
//! smoke / filter / sort exercise.
//!
//! Why this exists: production Winlink CMS rejects unregistered clients (see
//! the `project_cms_rejects_unknown_clients` memory + bd-tuxlink-0ic). The two
//! messages your inbox currently has are CMS "unknown client" rejection
//! responses — not real traffic. Until Tuxlink is a registered client, the
//! mailbox stays empty in normal operation.
//!
//! This binary writes ~35 realistic EmComm-shaped messages directly into the
//! mailbox dir, bypassing the CMS entirely. After running, click
//! Settings → Saved Searches → Maintenance → "Rebuild search index" so the
//! FTS5 index picks them up.
//!
//! Run:
//!   cargo run --manifest-path src-tauri/Cargo.toml --example seed_test_messages
//!
//! Optional: pass `--clear` first to wipe the existing mailbox before seeding.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tuxlink_lib::native_mailbox::Mailbox;
use tuxlink_lib::winlink::compose::compose_message;
use tuxlink_lib::winlink_backend::MailboxFolder;

fn mailbox_root() -> PathBuf {
    // Mirror Tauri's `app_data_dir().join("native-mbox")` on Linux. If you're
    // on a non-Linux host, override the parent by setting XDG_DATA_HOME.
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share"))
        })
        .expect("HOME not set");
    base.join("com.tuxlink.app").join("native-mbox")
}

fn main() {
    let clear = std::env::args().any(|a| a == "--clear");
    let root = mailbox_root();
    println!("Mailbox root: {}", root.display());

    if clear {
        for folder in ["inbox", "outbox", "sent", "archive"] {
            let dir = root.join(folder);
            if dir.exists() {
                println!("  ✕ wiping {}", dir.display());
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
    }

    let mbox = Mailbox::new(&root);

    let now_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before epoch")
        .as_secs();
    let day: u64 = 86_400;

    // (from, to, subject, body, days_ago, folder)
    // 35 messages spread across 8 senders, 4 form types, 3 folders, 30 days.
    let messages: Vec<(&str, &str, &str, &str, u64, MailboxFolder)> = vec![
        // ─── ICS-213 traffic (formal General Message) ──────────────────────
        (
            "KX5DD", "N7CPZ", "DAMAGE REPORT — Sector 7 powerlines down",
            "FORM: ICS-213\nTO: Net Control N7CPZ\nFROM: KX5DD (Field Team 3)\nSUBJECT: Sector 7 powerline incident\nPRIORITY: URGENT\n\nThree transmission poles snapped at mile marker 12 on Highway 304 NB. Transformer arc-fault, fire suppressed by responding fire crew at 1538L. No injuries. Lanes blocked NB; SB open. Need TO confirm receipt + relay to PSE&G ops.\n\nKX5DD/Field-3",
            0, MailboxFolder::Inbox,
        ),
        (
            "KX5DD", "N7CPZ", "Re: DAMAGE survey grid coordination",
            "FORM: ICS-213\nMSG: Acknowledged, proceeding with Sector 4 sweep by 1800L. Will report findings via this net.",
            3, MailboxFolder::Inbox,
        ),
        (
            "WX5RES", "N7CPZ", "ICS-213 — SKYWARN spotter activation",
            "FORM: ICS-213\nTO: Net Control\nFROM: WX5RES\nSUBJECT: SKYWARN spotter activation\nMSG: NWS-LWX requests spotter activation effective 1800Z. Local trained spotters to report wind gusts >40mph and any hail >quarter-size. Net to remain open until 0200Z.",
            1, MailboxFolder::Inbox,
        ),
        (
            "N5DRB", "N7CPZ", "ICS-213 evac shelter status",
            "FORM: ICS-213\nSHELTER: Lincoln HS\nCAPACITY: 240/500\nNEEDS: water, additional cots, MREs. Power on grid; no genset needed at this time.",
            2, MailboxFolder::Inbox,
        ),

        // ─── ICS-309 communications log ────────────────────────────────────
        (
            "K6BSA", "N7CPZ", "ICS-309 daily log 1800Z–2400Z",
            "FORM: ICS-309\nLOG: 1801Z KX5DD QSL · 1815Z WX5RES brief · 1832Z relay damage report sect 7 · 1908Z N5DRB shelter update · 2014Z KK7AAA QSL · 2207Z stand-down ack from EOC.",
            1, MailboxFolder::Inbox,
        ),

        // ─── DamageAssessment forms ────────────────────────────────────────
        (
            "KX5DD", "N7CPZ", "DMG-ASMT residential — Yarrow Pt",
            "FORM: DamageAssessment\nSECTOR: Yarrow Pt residential\nSTRUCTURES_AFFECTED: 6\nMAJOR_DAMAGE: 2 (roof failure)\nMINOR_DAMAGE: 4 (siding, gutters)\nNOTES: photos attached forthcoming; no structural collapse observed.",
            2, MailboxFolder::Inbox,
        ),
        (
            "KE9HHH", "N7CPZ", "Damage assessment — utility infrastructure",
            "FORM: DamageAssessment\nSECTOR: utility / substation alpha\nNOTES: feeder line down, substation isolated. PSE&G ETA 4h.",
            4, MailboxFolder::Inbox,
        ),

        // ─── Bulletin / weather / advisories ───────────────────────────────
        (
            "WX5RES", "N7CPZ", "Bulletin: severe weather watch 1800Z",
            "FORM: Bulletin\nSEVERE WEATHER WATCH issued by NWS-LWX effective 1800Z–0200Z. Damaging winds >60mph and hail to 1in possible. ARES nets activating.",
            1, MailboxFolder::Inbox,
        ),
        (
            "W1AW", "N7CPZ", "ARRL Bulletin 12 — propagation forecast",
            "FORM: Bulletin\nPropagation forecast for the week ahead: SFI 138, A=8, K=2. 10m FT8 openings expected mid-day across central US. HF nets should plan accordingly.",
            6, MailboxFolder::Inbox,
        ),
        (
            "N7XYZ", "N7CPZ", "Weather brief — ridge approach Sat",
            "Weekend ridge approach building over the basin. Expect Vy winds and reduced visibility on the ridge Sat afternoon. Field teams should monitor 146.520 secondary.",
            5, MailboxFolder::Inbox,
        ),

        // ─── Position reports ──────────────────────────────────────────────
        (
            "WX5RES", "N7CPZ", "Position report — mobile observer station",
            "FORM: Position\nGRID: CN87bm\nALT: 254m\nSPEED: 0 (stationary)\nNOTES: mobile observer parked at Hwy 304 mp 11, monitoring incident.",
            3, MailboxFolder::Inbox,
        ),
        (
            "KX5DD", "N7CPZ", "Position update Field Team 3",
            "FORM: Position\nGRID: CN87aq\nALT: 320m\nNOTES: Field Team 3 staged at staging area Bravo.",
            0, MailboxFolder::Inbox,
        ),

        // ─── Plain text traffic ────────────────────────────────────────────
        (
            "KK7AAA", "N7CPZ", "ACK Storm Net check-in",
            "Roger your check-in. Stand by for traffic. Storm Net active until 0200Z.",
            0, MailboxFolder::Inbox,
        ),
        (
            "KK7AAA", "N7CPZ", "Storm Net QSL — 1900Z window",
            "Confirming QSL Storm Net 1900Z window. Will key up again at 2000Z.",
            4, MailboxFolder::Inbox,
        ),
        (
            "N5DRB", "N7CPZ", "Shelter status update — Lincoln HS",
            "Lincoln HS shelter at 240/500 capacity. Water +2 cases delivered. Power stable.",
            2, MailboxFolder::Inbox,
        ),
        (
            "KE9HHH", "N7CPZ", "ACK — checked in for evening net",
            "Checked in for the evening net. Standing by on 146.520.",
            7, MailboxFolder::Inbox,
        ),
        (
            "N5DRB", "N7CPZ", "Cold front update — sustained winds",
            "Cold front passage at 0030Z. Sustained NW winds 25–35mph behind front. Spotters released.",
            8, MailboxFolder::Inbox,
        ),
        (
            "W1AW", "N7CPZ", "Drill exercise reminder Saturday 1700Z",
            "Drill exercise this Saturday 1700Z. Standard EmComm scenarios. All ARES members invited to participate.",
            10, MailboxFolder::Inbox,
        ),
        (
            "K6BSA", "N7CPZ", "Field Day prep — frequency coordination",
            "Field Day prep starting next weekend. Frequency coordination meeting Thursday 1900L.",
            12, MailboxFolder::Inbox,
        ),
        (
            "WX5RES", "N7CPZ", "Stand-down notice — all clear",
            "Stand-down notice. Severe weather watch expired. All clear. Spotters released. Thanks all.",
            1, MailboxFolder::Inbox,
        ),
        (
            "KX5DD", "N7CPZ", "Status report — Sector 4 sweep complete",
            "Sector 4 sweep complete. Minor flooding at the underpass; no structural damage observed. Heading to Sector 5 for staged inspection.",
            3, MailboxFolder::Inbox,
        ),

        // ─── Sent items (8) ────────────────────────────────────────────────
        (
            "N7CPZ", "KX5DD", "Re: DAMAGE survey grid coordination",
            "Acknowledged, proceed with Sector 4. Net Control standing by for status update by 1800L. K2N7CPZ Net Control",
            3, MailboxFolder::Sent,
        ),
        (
            "N7CPZ", "WX5RES", "Re: Bulletin SKYWARN — spotters activated",
            "Net Control acknowledged. Spotters activated 1800Z. Will rebroadcast on the hour. Standing by for severe reports.",
            1, MailboxFolder::Sent,
        ),
        (
            "N7CPZ", "N5DRB", "Re: Shelter status",
            "Copy shelter status. Will relay to ICS staging. Additional cot resupply ETA 1900L.",
            2, MailboxFolder::Sent,
        ),
        (
            "N7CPZ", "KE9HHH", "Re: Utility infrastructure damage",
            "Copy substation isolation. Logged. Forwarding to PSE&G ops via Net Control.",
            4, MailboxFolder::Sent,
        ),
        (
            "N7CPZ", "KX5DD", "Net Control roll call — ICS-213 ack",
            "FORM: ICS-213\nThis is N7CPZ Net Control. Roll call complete. KX5DD copy traffic for relay.",
            5, MailboxFolder::Sent,
        ),
        (
            "N7CPZ", "K6BSA", "Re: ICS-309 daily log",
            "Logged and archived. Thanks for the clean log. See you tomorrow.",
            1, MailboxFolder::Sent,
        ),
        (
            "N7CPZ", "WX5RES", "Re: Stand-down",
            "Net Control copies stand-down. All spotters released. Securing the net at 0235Z.",
            1, MailboxFolder::Sent,
        ),
        (
            "N7CPZ", "W1AW", "Drill scenario request",
            "Will participate in Saturday drill. Requesting wildfire-scenario propagation challenge.",
            10, MailboxFolder::Sent,
        ),

        // ─── Archive (5) ───────────────────────────────────────────────────
        (
            "KX5DD", "N7CPZ", "DRILL: archived damage exercise May",
            "DRILL EXERCISE — Mock damage report from May training. NOT a real incident. Archived for review.",
            25, MailboxFolder::Archive,
        ),
        (
            "WX5RES", "N7CPZ", "Archived weather summary — last week",
            "Weather summary for the previous week. Generally clear, light winds aloft. Archived for ops planning.",
            20, MailboxFolder::Archive,
        ),
        (
            "K6BSA", "N7CPZ", "Archived: ICS-309 log April monthly net",
            "FORM: ICS-309\nApril monthly net log. 14 check-ins, no traffic. Archived.",
            28, MailboxFolder::Archive,
        ),
        (
            "W1AW", "N7CPZ", "ARRL bulletin 10 — propagation review",
            "FORM: Bulletin\nMonthly propagation review. Solar activity averaged moderate. Archived.",
            18, MailboxFolder::Archive,
        ),
        (
            "N5DRB", "N7CPZ", "Old shelter exercise report",
            "Shelter exercise report from previous quarter. Lessons learned attached. Archived.",
            22, MailboxFolder::Archive,
        ),
    ];

    let mut count = 0;
    for (from, to, subj, body, days_ago, folder) in messages {
        let secs = now_secs - days_ago * day;
        let raw = compose_message(from, &[to], &[], subj, body, secs).to_bytes();
        match mbox.store(folder, &raw) {
            Ok(mid) => {
                count += 1;
                let folder_name = match folder {
                    MailboxFolder::Inbox => "inbox",
                    MailboxFolder::Outbox => "outbox",
                    MailboxFolder::Sent => "sent",
                    MailboxFolder::Archive => "archive",
                    _ => "other",
                };
                println!("  + {:>7} {}  {}", folder_name, mid.0, subj);
            }
            Err(e) => eprintln!("  ! failed: {e}"),
        }
    }

    println!();
    println!("Seeded {count} messages.");
    println!();
    println!("Next:");
    println!("  1. Open the app (or leave it running — these are picked up on next list-folder).");
    println!("  2. Settings → Saved Searches → Maintenance → Rebuild search index.");
    println!("  3. Try: from:KX5DD damage · form:ICS-213 · is:unread · weather · from:WX5RES");
}
