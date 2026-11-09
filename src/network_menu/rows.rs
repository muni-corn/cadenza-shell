//! Turning access points into menu rows.

use relm4::factory::FactoryVecDeque;

use crate::{
    network::{AccessPointSummary, get_strength_icon},
    widgets::menu_row::{MenuRow, MenuRowData, sync_rows},
};

/// Syncs the access point rows with the latest scan results, with the
/// connected network first and the rest by descending strength.
pub(super) fn update_ap_rows(
    rows: &mut FactoryVecDeque<MenuRow>,
    summaries: &[AccessPointSummary],
    active_ssid: Option<&str>,
) {
    let mut data: Vec<(bool, MenuRowData)> = summaries
        .iter()
        .map(|summary| {
            let active = Some(summary.ssid.as_str()) == active_ssid;
            (active, ap_row(summary, active))
        })
        .collect();
    // stable, so the strength order from the scan is kept
    data.sort_by_key(|(active, _)| !active);

    sync_rows(rows, data.into_iter().map(|(_, row)| row).collect());
}

/// How an access point is shown in the list, keyed by its ssid.
fn ap_row(summary: &AccessPointSummary, active: bool) -> MenuRowData {
    let status = if active {
        Some("Connected")
    } else if summary.is_saved() {
        Some("Saved")
    } else if !summary.security.is_open() {
        Some("Secured")
    } else {
        None
    };

    MenuRowData {
        key: summary.ssid.clone(),
        icon: Some(get_strength_icon(summary.strength).to_string()),
        title: summary.ssid.clone(),
        status,
        remove_tooltip: summary.is_saved().then_some("Forget this network"),
    }
}
