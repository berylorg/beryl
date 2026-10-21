use std::time::Duration;

use gpui::{
    App, ImageResourceDomain, ImageResourceDomainConfig, ImageResourceDrain, ImageResourceError,
    Task,
};

use crate::image_processing::MediaResourceLimits;

pub(crate) fn domain(
    limits: MediaResourceLimits,
    app: &mut App,
) -> Result<ImageResourceDomain, ImageResourceError> {
    let config = limits.configuration();
    app.image_resource_domain(ImageResourceDomainConfig {
        staging_buffers: config.upload_staging_buffers,
        staging_bytes: config.upload_staging_bytes,
        texture_entries: config.gpu_texture_entries,
        texture_bytes: config.gpu_texture_bytes as u64,
        window_pin_entries: config.window_pin_entries,
        window_pin_bytes: config.window_pin_gpu_bytes as u64,
    })
}

pub(crate) fn maintain(domain: ImageResourceDomain, app: &mut App) -> Task<()> {
    app.spawn(async move |cx| {
        loop {
            match domain.retirement_requested().await {
                Ok(()) => {}
                Err(ImageResourceError::Retired) => return,
                Err(error) => {
                    if !recover(&domain, error, cx).await {
                        return;
                    }
                    continue;
                }
            }
            loop {
                match cx.update(|app| app.poll_image_resource_retirement(&domain)) {
                    Ok(Ok(ImageResourceDrain::Drained)) => {
                        let _ = cx.update(native_progress);
                        break;
                    }
                    Ok(Ok(ImageResourceDrain::Pending)) => {
                        cx.background_executor()
                            .timer(Duration::from_millis(16))
                            .await;
                    }
                    Ok(Err(error)) => {
                        if !recover(&domain, error, cx).await {
                            return;
                        }
                        break;
                    }
                    Err(_) => return,
                }
            }
        }
    })
}

async fn recover(
    domain: &ImageResourceDomain,
    error: ImageResourceError,
    cx: &mut gpui::AsyncApp,
) -> bool {
    retain_failure(error, cx);
    if domain.recovery_requested().await.is_err() {
        return false;
    }
    cx.update(|app| {
        if app.has_global::<super::BootstrapLifetime>() {
            app.global_mut::<super::BootstrapLifetime>().media_failure = None;
        }
        native_progress(app);
    })
    .is_ok()
}

fn native_progress(app: &mut App) {
    let process = crate::running_owner::RunningProcessOwner::mounted_owner(app)
        .and_then(|owner| owner.upgrade())
        .and_then(|owner| owner.borrow().image_media_process());
    if let Some(process) = process {
        if let Err(error) = process.native_resources_changed() {
            tracing::error!(
                ?error,
                "image interests exhausted their native progress revision"
            );
        }
    }
    app.refresh_windows();
}

fn retain_failure(error: ImageResourceError, cx: &mut gpui::AsyncApp) {
    tracing::error!(?error, "native image retirement retains resource custody");
    let _ = cx.update(|app| {
        if app.has_global::<super::BootstrapLifetime>() {
            app.global_mut::<super::BootstrapLifetime>().media_failure = Some(error);
        }
    });
}
