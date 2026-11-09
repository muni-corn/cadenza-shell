use gtk4::{gdk, glib, prelude::*};

use crate::notifications::image::{ImageData, NotificationImage};

/// Shows `source` in `image`, hiding it if there's nothing to show.
///
/// Icon names that aren't in the theme are treated as missing so a broken
/// icon placeholder never appears.
pub fn show(image: &gtk4::Image, source: Option<&NotificationImage>) {
    let shown = match source {
        Some(NotificationImage::Pixels(data)) => match texture_from_pixels(data) {
            Some(texture) => {
                image.set_paintable(Some(&texture));
                true
            }
            None => false,
        },
        Some(NotificationImage::File(path)) if path.exists() => {
            image.set_from_file(Some(path));
            true
        }
        Some(NotificationImage::Icon(name)) if has_icon(name) => {
            image.set_icon_name(Some(name));
            true
        }
        _ => false,
    };
    image.set_visible(shown);
}

/// Whether the icon theme has an icon by this name.
pub fn has_icon(name: &str) -> bool {
    !name.is_empty()
        && gdk::Display::default()
            .is_some_and(|display| gtk4::IconTheme::for_display(&display).has_icon(name))
}

/// Uploads inline notification pixels as a texture.
fn texture_from_pixels(data: &ImageData) -> Option<gdk::Texture> {
    if !data.is_valid() {
        tracing::debug!("ignoring notification image data with an unsupported layout");
        return None;
    }

    let format = if data.has_alpha {
        gdk::MemoryFormat::R8g8b8a8
    } else {
        gdk::MemoryFormat::R8g8b8
    };
    let texture = gdk::MemoryTexture::new(
        data.width,
        data.height,
        format,
        &glib::Bytes::from(&data.data[..]),
        data.rowstride as usize,
    );
    Some(texture.upcast())
}
