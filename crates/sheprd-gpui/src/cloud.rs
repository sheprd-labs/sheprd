//! Cloud machines as devices: `sheprd_remote::cloud`, plus what the window
//! drives here: the background jobs that add machines with their progress
//! and toasts, the batches agents ask for, and the sandbox image builds.
// i18n: migrated

pub(crate) mod batch;
pub(crate) mod image;
mod jobs;
pub(crate) mod status;
#[cfg(test)]
mod tests;
pub(crate) mod worker;

pub(crate) use jobs::Jobs;
pub(crate) use sheprd_remote::cloud::*;

use crate::config::{Feature, Features};

/// The providers this launch offers: those built in whose `[features]`
/// switch is on, while `cloud_providers` is. Read once from the startup
/// config, so a change takes effect when Sheprd next starts. Tests see every
/// provider built in, whatever the config on the machine running them says,
/// unless they choose others with [`enable_for_test`].
pub(crate) fn enabled() -> &'static [CloudProvider] {
    #[cfg(test)]
    return TEST_ENABLED
        .with(|enabled| enabled.get())
        .unwrap_or(CloudProvider::ALL);
    #[cfg(not(test))]
    {
        static ENABLED: std::sync::OnceLock<Vec<CloudProvider>> = std::sync::OnceLock::new();
        ENABLED.get_or_init(|| {
            let features = crate::config::Config::load_startup()
                .map(|config| config.features)
                .unwrap_or_default();
            enabled_in(&features)
        })
    }
}

#[cfg(test)]
thread_local! {
    static TEST_ENABLED: std::cell::Cell<Option<&'static [CloudProvider]>> =
        const { std::cell::Cell::new(None) };
}

/// Turns on only `providers` for this test thread, which GPUI tests and their
/// executors share, until the guard drops.
#[cfg(test)]
pub(crate) fn enable_for_test(providers: &[CloudProvider]) -> EnabledForTest {
    let providers: &'static [CloudProvider] = Box::leak(providers.to_vec().into_boxed_slice());
    EnabledForTest(TEST_ENABLED.with(|enabled| enabled.replace(Some(providers))))
}

/// Restores the providers tests see; see [`enable_for_test`].
#[cfg(test)]
pub(crate) struct EnabledForTest(Option<&'static [CloudProvider]>);

#[cfg(test)]
impl Drop for EnabledForTest {
    fn drop(&mut self) {
        TEST_ENABLED.with(|enabled| enabled.set(self.0));
    }
}

/// Whether this launch offers `provider`; see [`enabled`].
pub(crate) fn is_enabled(provider: CloudProvider) -> bool {
    enabled().contains(&provider)
}

/// The providers `features` turns on, in display order.
fn enabled_in(features: &Features) -> Vec<CloudProvider> {
    if !features.cloud_providers {
        return Vec::new();
    }
    CloudProvider::ALL
        .iter()
        .copied()
        .filter(|&provider| features.enabled(switch(provider)))
        .collect()
}

/// The `[features]` flag that turns `provider` on or off.
fn switch(provider: CloudProvider) -> Feature {
    match provider {
        #[cfg(feature = "azure")]
        CloudProvider::Azure => Feature::CloudAzure,
        #[cfg(feature = "boat")]
        CloudProvider::Boat => Feature::CloudBoat,
        #[cfg(feature = "boxd")]
        CloudProvider::Boxd => Feature::CloudBoxd,
        #[cfg(feature = "coder")]
        CloudProvider::Coder => Feature::CloudCoder,
        #[cfg(feature = "container")]
        CloudProvider::Container => Feature::CloudContainer,
        #[cfg(feature = "daytona")]
        CloudProvider::Daytona => Feature::CloudDaytona,
        #[cfg(feature = "opensandbox")]
        CloudProvider::OpenSandbox => Feature::CloudOpenSandbox,
        #[cfg(feature = "railway")]
        CloudProvider::Railway => Feature::CloudRailway,
        #[cfg(feature = "sprites")]
        CloudProvider::Sprites => Feature::CloudSprites,
    }
}

/// Every minute, stop machines left idle: Azure VMs past `auto_stop`, and
/// idle containers. Each reads its table as the first frame does, with no
/// maintenance, and only when one of its devices is saved. These run even
/// while the provider is turned off in `[features]`, so a hidden machine left
/// running still stops instead of costing money.
pub(crate) fn install_idle_checks(cx: &mut gpui::App) {
    #[cfg(feature = "azure")]
    every(cx, std::time::Duration::from_secs(60), || {
        crate::azure::check_all_idle_devices(|| {
            crate::config::Config::load_startup()
                .ok()
                .map(|config| config.azure)
        })
    });
    #[cfg(feature = "container")]
    every(cx, crate::container::idle::CHECK, || {
        crate::container::idle::check_all(|| {
            crate::config::Config::load_startup()
                .ok()
                .map(|config| config.container)
        })
    });
    let _ = cx;
}

#[cfg(any(feature = "azure", feature = "container"))]
fn every(cx: &mut gpui::App, interval: std::time::Duration, check: impl Fn() + Send + 'static) {
    let timer = cx.background_executor().clone();
    cx.background_executor()
        .spawn(async move {
            loop {
                timer.timer(interval).await;
                check();
            }
        })
        .detach();
}
