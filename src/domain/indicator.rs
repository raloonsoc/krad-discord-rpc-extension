//! What the titlebar button says, from the extension's own switches.
//! Pure: pause, the master switch and the IPC connection are the whole input.

/// The titlebar button's highlight and tooltip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Indicator {
    /// Highlighted while the presence is actually being shared.
    pub(crate) active: bool,
    pub(crate) tooltip: String,
}

const SHARING: &str = "Discord RPC: sharing";
const PAUSED: &str = "Discord RPC: paused";
const NOT_RUNNING: &str = "Discord RPC: Discord not running";

/// The button for these switches. Pause and the master switch win over a lost
/// connection: the user asked for no presence, whether Discord runs or not.
#[must_use]
pub(crate) fn indicator(paused: bool, enabled: bool, connected: bool) -> Indicator {
    let (active, tooltip) = if paused || !enabled {
        (false, PAUSED)
    } else if !connected {
        (false, NOT_RUNNING)
    } else {
        (true, SHARING)
    };
    Indicator {
        active,
        tooltip: tooltip.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_connected_unpaused_extension_shares() {
        let shown = indicator(false, true, true);
        assert!(shown.active);
        assert_eq!(shown.tooltip, "Discord RPC: sharing");
    }

    #[test]
    fn pause_and_the_master_switch_read_as_paused() {
        for (paused, enabled) in [(true, true), (false, false), (true, false)] {
            let shown = indicator(paused, enabled, true);
            assert!(!shown.active, "{paused} {enabled}");
            assert_eq!(shown.tooltip, "Discord RPC: paused");
        }
    }

    #[test]
    fn pausing_wins_over_a_lost_connection() {
        let shown = indicator(true, true, false);
        assert!(!shown.active);
        assert_eq!(shown.tooltip, "Discord RPC: paused");
    }

    #[test]
    fn a_missing_connection_is_not_sharing() {
        let shown = indicator(false, true, false);
        assert!(!shown.active);
        assert_eq!(shown.tooltip, "Discord RPC: Discord not running");
    }
}
