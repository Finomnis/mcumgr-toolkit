use crate::commands::image::ImageState;

/// The current run state of the given image.
pub enum ImageRunState<'a> {
    /// The given slot is likely the current and future
    /// image on the system.
    Stable(&'a ImageState),
    /// The image will change at next boot.
    Pending {
        /// Currently running/active image, if known.
        current: Option<&'a ImageState>,
        /// The slot that will run after next boot.
        ///
        /// Examine `next.permanent` to see whether
        /// the image will boot in testing or permanent
        /// mode.
        next: &'a ImageState,
    },
    /// A new image is currently being tested.
    ///
    /// Unless it gets confirmed, it will be
    /// rolled back at next boot.
    Testing {
        /// The currently tested image.
        current: &'a ImageState,
        /// The image that will get rolled back to.
        fallback: &'a ImageState,
    },
    /// The current state is not easily determinable.
    ///
    /// Will contain an educated guess if possible,
    /// but by no means this guess should be relied upon.
    Unknown(Option<&'a ImageState>),
}

/// Analyze the flags of the image slots with the given image ID and determine the most likely run state.
pub fn analyze(image_state: &[ImageState], image_id: u32) -> ImageRunState<'_> {
    fn find_unique(
        image_id: u32,
        image_state: &[ImageState],
        predicate: impl Fn(&ImageState) -> bool,
    ) -> Option<Option<&'_ ImageState>> {
        let mut matches = image_state
            .iter()
            .filter(|img| img.image == image_id)
            .filter(|img| predicate(img));

        let first = matches.next();

        match matches.next() {
            Some(_) => None,     // invalid: duplicate
            None => Some(first), // valid: zero or one
        }
    }

    let active = match find_unique(image_id, image_state, |img| img.active) {
        Some(value) => value,
        None => return ImageRunState::Unknown(None),
    };

    let confirmed = match find_unique(image_id, image_state, |img| img.confirmed) {
        Some(value) => value,
        None => return ImageRunState::Unknown(None),
    };

    let pending = match find_unique(image_id, image_state, |img| img.pending) {
        Some(value) => value,
        None => return ImageRunState::Unknown(None),
    };

    if let Some(pending) = pending {
        return ImageRunState::Pending {
            current: active.or(confirmed),
            next: pending,
        };
    }

    match (active, confirmed) {
        (Some(active), Some(confirmed)) if active.slot == confirmed.slot => {
            ImageRunState::Stable(active)
        }
        (Some(active), Some(confirmed)) => ImageRunState::Testing {
            current: active,
            fallback: confirmed,
        },
        (Some(active), None) => ImageRunState::Stable(active),
        (None, Some(confirmed)) => ImageRunState::Stable(confirmed),
        (None, None) => {
            // Probably MCUboot with image infos disabled, guess slot 0
            ImageRunState::Unknown(
                image_state
                    .iter()
                    .filter(|img| img.image == image_id)
                    .find(|img| img.slot == 0),
            )
        }
    }
}
