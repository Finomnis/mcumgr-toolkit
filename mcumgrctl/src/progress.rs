use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

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
                let progress = multiprogress.add(ProgressBar::new(total));

                if let Some(message) = &message {
                    progress.set_message(message.to_string());
                }

                let style = ProgressStyle::with_template(
                    "{pct%:>4.#f330f3} {pretty_bar} {cur_total_bytes:.green} • {decimal_bytes_per_sec:.red} • {msg}", //"{msg} {wide_bar} {decimal_bytes:>9} / {decimal_total_bytes:9} ({decimal_bytes_per_sec:9})",
                )
                .unwrap()
                .with_key("pretty_bar", widgets::pretty_bar)
                .with_key("cur_total_bytes", widgets::cur_total_bytes)
                .with_key("short_elapsed", widgets::elapsed_time)
                .with_key("pct%", widgets::percent);

                progress.set_style(style);

                progress
            });

            progress.set_length(total);
            progress.set_position(current);
            true
        };

        let result = action(Some(&mut callback));

        if let Some(progress) = progress {
            progress.abandon();
            // multiprogress.remove(&progress);
        }

        result
    } else {
        action(None)
    }
}

mod widgets {
    use std::{
        fmt::{self, Write},
        time::Duration,
    };

    use console::StyledObject;
    use indicatif::ProgressState;
    use unit_prefix::NumberPrefix;

    struct ShortDuration(pub Duration);

    impl fmt::Display for ShortDuration {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            let mut t = self.0.as_secs();
            let seconds = t % 60;
            t /= 60;
            let minutes = t % 60;
            t /= 60;
            let hours = t % 24;
            t /= 24;
            if t > 0 {
                let days = t;
                write!(f, "{days}d {hours:02}:{minutes:02}:{seconds:02}")
            } else if hours > 0 {
                write!(f, "{hours:02}:{minutes:02}:{seconds:02}")
            } else {
                write!(f, "{minutes:02}:{seconds:02}")
            }
        }
    }

    pub(super) fn percent(state: &ProgressState, w: &mut dyn Write) {
        write!(w, "{}%", (state.fraction() * 100.0).round() as u8).ok();
    }

    pub(super) fn elapsed_time(state: &ProgressState, w: &mut dyn Write) {
        write!(w, "{}", ShortDuration(state.elapsed())).ok();
    }

    pub(super) fn cur_total_bytes(state: &ProgressState, w: &mut dyn Write) {
        let pos = state.pos() as f64;
        if let Some(len) = state.len() {
            let len = len as f64;
            match NumberPrefix::decimal(len) {
                NumberPrefix::Standalone(len) => write!(w, "{pos:.0}/{len:.0} B"),
                NumberPrefix::Prefixed(prefix, len_formatted) => {
                    let multiplier = len_formatted / len;
                    let pos = pos * multiplier;
                    write!(w, "{pos:.1}/{len_formatted:.1} {prefix}B")
                }
            }
        } else {
            match NumberPrefix::decimal(pos) {
                NumberPrefix::Standalone(number) => write!(w, "{number:.0} B"),
                NumberPrefix::Prefixed(prefix, number) => write!(w, "{number:.2} {prefix}B"),
            }
        }
        .ok();
    }

    pub(super) fn pretty_bar(state: &ProgressState, w: &mut dyn Write) {
        const BAR_SIZE: u64 = 40;

        type ColorFn = &'static dyn Fn(StyledObject<String>) -> StyledObject<String>;
        const COLOR_INACTIVE: ColorFn = &|s| s.color256(237);
        const COLOR_ACTIVE: ColorFn = &|s| s.true_color(249, 38, 114);
        const COLOR_FINISHED: ColorFn = &|s| s.green();

        fn write_bar(
            w: &mut dyn Write,
            len: u64,
            color: &dyn Fn(StyledObject<String>) -> StyledObject<String>,
        ) {
            let bar = std::iter::repeat_n('━', len as usize).collect::<String>();
            write!(w, "{}", color(console::style(bar))).ok();
        }

        if state.pos() == 0 {
            write_bar(w, BAR_SIZE, COLOR_INACTIVE);
            return;
        }

        if state.is_finished() {
            write_bar(w, BAR_SIZE, COLOR_FINISHED);
            return;
        }

        if let Some(len) = state.len()
            && len == state.pos()
        {
            write_bar(w, BAR_SIZE, COLOR_ACTIVE);
            return;
        }

        if let Some(len) = state.len() {
            let step = (2 * BAR_SIZE * state.pos() / len).clamp(0, 2 * BAR_SIZE - 1);

            let bar_left_len = step / 2;
            let bar_right_len = BAR_SIZE - 1 - bar_left_len;

            write_bar(w, bar_left_len, COLOR_ACTIVE);
            if step % 2 == 0 {
                write!(w, "{}", COLOR_INACTIVE(console::style(String::from("╺")))).ok();
            } else {
                write!(w, "{}", COLOR_ACTIVE(console::style(String::from("╸")))).ok();
            }
            write_bar(w, bar_right_len, COLOR_INACTIVE);
        } else {
            write_bar(w, BAR_SIZE, COLOR_INACTIVE);
        }
    }
}
