use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressStyle};

pub fn with_progress_bar<T>(
    multiprogress: &MultiProgress,
    show: bool,
    message: Option<&str>,
    action: impl FnOnce(Option<&mut dyn FnMut(u64, u64) -> bool>) -> T,
) -> T {
    if show {
        let mut progress = None;

        let mut callback = |current, total| {
            let progress = progress.get_or_insert_with(|| {
                let progress = multiprogress.add(ProgressBar::new(total)).with_finish(ProgressFinish::AndClear);

                if let Some(message) = &message {
                    progress.set_message(message.to_string());
                }

                progress.set_style(
                ProgressStyle::with_template(
                    "{msg} {wide_bar} {decimal_bytes:>9} / {decimal_total_bytes:9} ({decimal_bytes_per_sec:9})",
                )
                .unwrap());

                progress
            });

            progress.set_length(total);
            progress.set_position(current);
            true
        };

        let result = action(Some(&mut callback));

        if let Some(progress) = progress {
            progress.finish_and_clear();
            multiprogress.remove(&progress);
        }

        result
    } else {
        action(None)
    }
}
