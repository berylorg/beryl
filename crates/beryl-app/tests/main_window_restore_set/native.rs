use super::*;
use beryl_app::theme_runtime::{
    AppearanceCoordinator, AppearanceCoordinatorConfig, AppearanceGeneration,
    GpuiAppearanceWindowSet,
};
use gpui::{App, Application, AsyncApp};
use std::{
    cell::RefCell,
    num::NonZeroUsize,
    rc::Rc,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[path = "native/support.rs"]
mod native_support;
use native_support::*;

#[path = "native/cancellation.rs"]
mod cancellation;
#[path = "native/failures.rs"]
mod failures;
#[path = "native/retained.rs"]
mod retained;
#[path = "native/success.rs"]
mod success;
