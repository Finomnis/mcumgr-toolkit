/// MCUboot image parser
mod image;

pub use image::{ImageInfo, ImageParseError, ImageVersion, get_image_info};
