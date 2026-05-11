use std::{path::PathBuf, rc::Rc};

use gpui::{AsyncApp, Context, WeakEntity};

use super::transcript_branch_menu_state::TranscriptImageMenuTarget;

type NoticeSink<T> = dyn Fn(&mut T, &'static str, String) + 'static;

pub(crate) fn copy_transcript_image_to_clipboard<T: 'static>(
    target: &TranscriptImageMenuTarget,
    cx: &mut Context<T>,
) {
    cx.write_to_clipboard(target.clipboard_item());
}

pub(crate) fn save_transcript_image_as<T: 'static>(
    target: TranscriptImageMenuTarget,
    notice_sink: impl Fn(&mut T, &'static str, String) + 'static,
    cx: &mut Context<T>,
) {
    let bytes = target.bytes_arc();
    let suggested_name = target.suggested_save_filename();
    let initial_directory = transcript_image_save_initial_directory();
    let picked_path = cx.prompt_for_new_path(&initial_directory, Some(&suggested_name));
    let write_executor = cx.background_executor().clone();
    let notice_sink: Rc<NoticeSink<T>> = Rc::new(notice_sink);

    cx.spawn(move |view: WeakEntity<T>, cx: &mut AsyncApp| {
        let mut cx = cx.clone();
        let notice_sink = notice_sink.clone();
        async move {
            let selected_path = match picked_path.await {
                Ok(Ok(Some(path))) => target.save_path_with_default_extension(path),
                Ok(Ok(None)) => return,
                Ok(Err(error)) => {
                    report_image_save_failure(
                        &view,
                        &mut cx,
                        notice_sink.as_ref(),
                        format!("Beryl could not open the image save picker: {error}"),
                    );
                    return;
                }
                Err(error) => {
                    report_image_save_failure(
                        &view,
                        &mut cx,
                        notice_sink.as_ref(),
                        format!("Beryl could not complete the image save picker: {error}"),
                    );
                    return;
                }
            };

            let write_path = selected_path.clone();
            let write_task = write_executor.spawn(async move {
                std::fs::write(&write_path, bytes.as_ref()).map(|_| write_path)
            });
            if let Err(error) = write_task.await {
                report_image_save_failure(
                    &view,
                    &mut cx,
                    notice_sink.as_ref(),
                    format!("Beryl could not write {}: {error}", selected_path.display()),
                );
            }
        }
    })
    .detach();
    cx.notify();
}

fn report_image_save_failure<T: 'static>(
    view: &WeakEntity<T>,
    cx: &mut AsyncApp,
    notice_sink: &NoticeSink<T>,
    detail: String,
) {
    let _ = view.update(cx, |view, cx| {
        notice_sink(view, "Image save failed", detail);
        cx.notify();
    });
}

fn transcript_image_save_initial_directory() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
