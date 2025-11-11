use std::borrow::Cow;

use indicatif::{ProgressBar, ProgressStyle};

pub fn with_progress_bar<T>(
    show: bool,
    size_hint: Option<u64>,
    message: Option<impl Into<Cow<'static, str>>>,
    action: impl FnOnce(Option<&mut dyn FnMut(u64, u64) -> bool>) -> T,
) -> T {
    if show {
        let progress = if let Some(size_hint) = size_hint {
            ProgressBar::new(size_hint)
        } else {
            ProgressBar::no_length()
        };
        if let Some(message) = message {
            progress.set_message(message);
        }
        progress.set_style(
            ProgressStyle::with_template(
                "{msg} {wide_bar} {decimal_bytes:>9} / {decimal_total_bytes:9} ({decimal_bytes_per_sec:9})",
            )
            .unwrap(),
        );
        let result = action(Some(&mut |current, total| {
            progress.set_length(total);
            progress.set_position(current);
            true
        }));
        progress.finish();
        result
    } else {
        action(None)
    }
}
