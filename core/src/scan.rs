use crate::link::{CAMERA_REFUSED, Reject, parse_link};

pub const CAMERA_FAILED: &str = "The camera could not start. Try again, or paste the link.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanState {
    Starting,
    Permission,
    Preview,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanStatus {
    Code,
    Permission,
    Failed,
    Cancelled,
}

impl ScanStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "code" => Some(Self::Code),
            "permission" => Some(Self::Permission),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanError {
    Permission,
    Failed,
    Rejected(Reject),
}

impl ScanError {
    pub fn message(self) -> &'static str {
        match self {
            Self::Permission => CAMERA_REFUSED,
            Self::Failed => CAMERA_FAILED,
            Self::Rejected(reject) => reject.message(),
        }
    }

    pub fn retry(self) -> bool {
        matches!(self, Self::Permission | Self::Failed)
    }
}

pub struct Scan {
    state: ScanState,
}

impl Scan {
    pub fn new() -> Self {
        Self {
            state: ScanState::Starting,
        }
    }

    pub fn state(&self) -> ScanState {
        self.state
    }

    pub fn apply(
        &mut self,
        status: Option<ScanStatus>,
        text: Option<String>,
    ) -> Result<Option<String>, ScanError> {
        match status {
            Some(ScanStatus::Code) => {
                self.state = ScanState::Preview;
                let text = text.ok_or(ScanError::Failed)?;
                let text = text.trim();
                parse_link(text).map_err(ScanError::Rejected)?;
                Ok(Some(text.to_owned()))
            }
            Some(ScanStatus::Permission) => {
                self.state = ScanState::Permission;
                Err(ScanError::Permission)
            }
            Some(ScanStatus::Cancelled) => Ok(None),
            Some(ScanStatus::Failed) | None => {
                self.state = ScanState::Failed;
                Err(ScanError::Failed)
            }
        }
    }

    pub fn retry(&mut self) {
        self.state = ScanState::Starting;
    }
}

impl Default for Scan {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::LINK_REJECTED;

    const ID: &str = "4d92adf6fd9076ddd383c22e08f6ac6fb0ce48210b69c0f954e7e199b4d96ed6";

    fn link() -> String {
        format!("handboard://c#v=1&e={ID}&f=f0123456789ab&c=K7QM3XRD2P")
    }

    fn decoded(text: Option<&str>) -> (Option<ScanStatus>, Option<String>) {
        match text {
            Some(text) => (Some(ScanStatus::Code), Some(text.to_owned())),
            None => (Some(ScanStatus::Cancelled), None),
        }
    }

    #[test]
    fn a_scan_starts_and_a_decoded_link_is_returned() {
        let mut scan = Scan::new();
        assert_eq!(scan.state(), ScanState::Starting);
        let (status, text) = decoded(Some(&link()));
        assert_eq!(scan.apply(status, text).unwrap(), Some(link()));
        assert_eq!(scan.state(), ScanState::Preview);
        let (status, text) = decoded(Some(&format!("  {}\n", link())));
        assert_eq!(scan.apply(status, text).unwrap(), Some(link()));
    }

    #[test]
    fn a_foreign_code_is_rejected_with_the_paste_message() {
        let mut scan = Scan::new();
        let (status, text) = decoded(Some("https://example.test/"));
        let error = scan.apply(status, text).unwrap_err();
        assert_eq!(error, ScanError::Rejected(Reject::Scheme));
        assert_eq!(error.message(), LINK_REJECTED);
        assert!(!error.retry());
        assert_eq!(scan.state(), ScanState::Preview);
    }

    #[test]
    fn a_refused_permission_is_retriable_and_says_the_camera_is_needed() {
        let mut scan = Scan::new();
        let error = scan.apply(Some(ScanStatus::Permission), None).unwrap_err();
        assert_eq!(scan.state(), ScanState::Permission);
        assert_eq!(error.message(), CAMERA_REFUSED);
        assert!(error.retry());
        scan.retry();
        assert_eq!(scan.state(), ScanState::Starting);
    }

    #[test]
    fn a_camera_failure_is_retriable() {
        let mut scan = Scan::new();
        let error = scan.apply(Some(ScanStatus::Failed), None).unwrap_err();
        assert_eq!(scan.state(), ScanState::Failed);
        assert_eq!(error.message(), CAMERA_FAILED);
        assert!(error.retry());
        scan.retry();
        assert_eq!(scan.state(), ScanState::Starting);
    }

    #[test]
    fn cancelling_and_unknown_replies_do_not_leak_a_code() {
        let mut scan = Scan::new();
        assert_eq!(scan.apply(Some(ScanStatus::Cancelled), None).unwrap(), None);
        let error = scan.apply(None, Some(link())).unwrap_err();
        assert_eq!(error, ScanError::Failed);
        assert!(!error.message().contains("K7QM3XRD2P"));
    }

    #[test]
    fn the_status_names_are_the_plugin_protocol() {
        assert_eq!(ScanStatus::parse("code"), Some(ScanStatus::Code));
        assert_eq!(
            ScanStatus::parse("permission"),
            Some(ScanStatus::Permission)
        );
        assert_eq!(ScanStatus::parse("failed"), Some(ScanStatus::Failed));
        assert_eq!(ScanStatus::parse("cancelled"), Some(ScanStatus::Cancelled));
        assert_eq!(ScanStatus::parse("granted"), None);
    }
}
