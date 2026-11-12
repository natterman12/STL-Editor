//! The Support Fins engine host: runs `SupportFinsEngine.<name>(...args)` in some JavaScript engine.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostKind {
    Webview,
    Browser,
    Boa,
    None,
}

impl HostKind {
    pub fn as_str(self) -> &'static str {
        match self {
            HostKind::Webview => "webview",
            HostKind::Browser => "browser",
            HostKind::Boa => "boa",
            HostKind::None => "none",
        }
    }
}

pub trait FinsHost: Send + Sync {
    /// `SupportFinsEngine.<name>(...args)`, string result.
    fn call(&self, name: &str, args: &[&str]) -> Result<String, String>;
    /// `typeof SupportFinsEngine.<name> === 'function'`.
    fn has(&self, name: &str) -> bool;
    fn host_kind(&self) -> HostKind;
}

pub struct NullHost;

impl FinsHost for NullHost {
    fn call(&self, _name: &str, _args: &[&str]) -> Result<String, String> {
        Err("Support Fins isn't available in this build.".into())
    }
    fn has(&self, _name: &str) -> bool {
        false
    }
    fn host_kind(&self) -> HostKind {
        HostKind::None
    }
}
