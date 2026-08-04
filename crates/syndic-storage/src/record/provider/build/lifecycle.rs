/// Publication state of one bounded provider-item build frontier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderItemBuildLifecycle {
    Staging,
    Sealed,
}
