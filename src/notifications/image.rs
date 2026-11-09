use std::path::PathBuf;

/// Raw pixels from the `image-data` hint, in the spec's `(iiibiiay)` layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageData {
    pub width: i32,
    pub height: i32,
    /// Bytes per row, which may include padding.
    pub rowstride: i32,
    pub has_alpha: bool,
    pub bits_per_sample: i32,
    pub channels: i32,
    pub data: Vec<u8>,
}

impl ImageData {
    /// Whether the layout is one we can upload: 8-bit rgb or rgba with
    /// enough bytes for every row.
    pub fn is_valid(&self) -> bool {
        let expected_channels = if self.has_alpha { 4 } else { 3 };
        if self.width <= 0
            || self.height <= 0
            || self.bits_per_sample != 8
            || self.channels != expected_channels
            || self.rowstride < self.width * self.channels
        {
            return false;
        }

        // the last row doesn't need its padding
        let needed = self.rowstride as usize * (self.height as usize - 1)
            + self.width as usize * self.channels as usize;
        self.data.len() >= needed
    }
}

/// An image shown with a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationImage {
    /// Pixels sent inline with the notification, boxed since they're much
    /// bigger than the other variants.
    Pixels(Box<ImageData>),
    /// An image file on disk.
    File(PathBuf),
    /// A name in the icon theme.
    Icon(String),
}

impl NotificationImage {
    /// Interprets a value that the spec allows to be a file path, a
    /// `file://` uri or an icon name. Returns `None` for an empty value.
    pub fn from_path_or_icon(value: &str) -> Option<Self> {
        if value.is_empty() {
            None
        } else if let Some(path) = value.strip_prefix("file://") {
            Some(Self::File(PathBuf::from(path)))
        } else if value.starts_with('/') {
            Some(Self::File(PathBuf::from(value)))
        } else {
            Some(Self::Icon(value.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixels(width: i32, height: i32, has_alpha: bool, len: usize) -> ImageData {
        let channels = if has_alpha { 4 } else { 3 };
        ImageData {
            width,
            height,
            rowstride: width * channels,
            has_alpha,
            bits_per_sample: 8,
            channels,
            data: vec![0; len],
        }
    }

    #[test]
    fn paths_uris_and_icon_names_are_told_apart() {
        assert_eq!(
            NotificationImage::from_path_or_icon("/tmp/a.png"),
            Some(NotificationImage::File("/tmp/a.png".into()))
        );
        assert_eq!(
            NotificationImage::from_path_or_icon("file:///tmp/a.png"),
            Some(NotificationImage::File("/tmp/a.png".into()))
        );
        assert_eq!(
            NotificationImage::from_path_or_icon("firefox"),
            Some(NotificationImage::Icon("firefox".into()))
        );
        assert_eq!(NotificationImage::from_path_or_icon(""), None);
    }

    #[test]
    fn complete_rgba_pixels_are_valid() {
        assert!(pixels(2, 2, true, 16).is_valid());
        assert!(pixels(2, 2, false, 12).is_valid());
    }

    #[test]
    fn short_or_odd_pixels_are_invalid() {
        assert!(!pixels(2, 2, true, 15).is_valid());
        let mut sixteen_bit = pixels(2, 2, true, 32);
        sixteen_bit.bits_per_sample = 16;
        assert!(!sixteen_bit.is_valid());
    }
}
