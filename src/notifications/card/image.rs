use gtk4::{gdk, glib, prelude::*};

use crate::notifications::{
    image::{ImageData, NotificationImage},
    markup::BodyImage,
};

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

/// Builds the widget for an image embedded in a notification body: the
/// picture for local files, or its alt text otherwise (e.g. for remote
/// images, which aren't fetched).
pub fn body_image(body_image: &BodyImage) -> gtk4::Widget {
    let path = match NotificationImage::from_path_or_icon(&body_image.src) {
        Some(NotificationImage::File(path)) if path.exists() => Some(path),
        _ => None,
    };

    match path {
        Some(path) => {
            let picture = gtk4::Picture::for_filename(path);
            picture.set_can_shrink(true);
            picture.set_content_fit(gtk4::ContentFit::Contain);
            picture.set_height_request(BODY_IMAGE_HEIGHT);
            picture.set_alternative_text(Some(&body_image.alt));
            picture.add_css_class("body-image");
            picture.upcast()
        }
        None => gtk4::Label::builder()
            .label(&body_image.alt)
            .halign(gtk4::Align::Start)
            .css_classes(["body", "dim"])
            .visible(!body_image.alt.is_empty())
            .build()
            .upcast(),
    }
}

/// Height of images embedded in notification bodies.
const BODY_IMAGE_HEIGHT: i32 = 120;

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
