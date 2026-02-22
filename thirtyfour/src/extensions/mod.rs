/// Extensions for working with Firefox Addons.
pub mod addons;
/// Extensions for Chrome Devtools Protocol
pub mod cdp;
// ElementQuery and ElementWaiter interfaces.
/// WebDriver BiDi bidirectional protocol support.
#[cfg(feature = "bidi")]
pub mod bidi;
pub mod query;
