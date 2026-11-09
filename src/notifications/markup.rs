//! Turning notification body markup into something a gtk label can show.
//!
//! The spec allows a small html-like subset: `<b>`, `<i>`, `<u>`,
//! `<a href="...">` and `<img src="..." alt="..."/>`. Images are pulled out
//! to be shown separately, everything else is dropped while keeping its
//! text, and text is escaped so stray `<` and `&` can't break pango.

/// A notification body split into label markup and the images it embeds.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Body {
    /// Gtk label markup that's safe to render.
    pub markup: String,
    /// Images from `<img>` tags, which a label can't show inline.
    pub images: Vec<BodyImage>,
}

/// An `<img>` tag from a notification body.
#[derive(Debug, PartialEq, Eq)]
pub struct BodyImage {
    /// A file path or uri.
    pub src: String,
    /// Text to show if the image can't be.
    pub alt: String,
}

/// Parses a notification body.
///
/// If the markup still isn't valid after sanitizing (e.g. mismatched tags),
/// the body is shown as escaped plain text instead, without images.
pub fn parse_body(body: &str) -> Body {
    let mut images = Vec::new();
    let markup = sanitize(body, &mut images);
    if is_valid_markup(&markup) {
        Body { markup, images }
    } else {
        Body {
            markup: glib::markup_escape_text(body).to_string(),
            images: Vec::new(),
        }
    }
}

/// Rewrites `body` keeping only the supported tags, collecting images.
fn sanitize(body: &str, images: &mut Vec<BodyImage>) -> String {
    let mut out = String::with_capacity(body.len());
    let mut rest = body;

    while let Some(start) = rest.find('<') {
        push_text(&mut out, &rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else {
            // an unterminated tag is just text
            push_text(&mut out, &rest[start..]);
            return out;
        };
        push_tag(&mut out, &after[..end], images);
        rest = &after[end + 1..];
    }

    push_text(&mut out, rest);
    out
}

/// Emits the sanitized form of the tag whose contents (between `<` and `>`)
/// are `tag`.
fn push_tag(out: &mut String, tag: &str, images: &mut Vec<BodyImage>) {
    let (closing, tag) = match tag.strip_prefix('/') {
        Some(tag) => (true, tag),
        None => (false, tag),
    };
    let tag = tag.trim().trim_end_matches('/').trim_end();
    let name = tag
        .split(|c: char| !c.is_ascii_alphanumeric())
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();

    match (name.as_str(), closing) {
        ("b" | "i" | "u", false) => out.push_str(&format!("<{name}>")),
        ("b" | "i" | "u" | "a", true) => out.push_str(&format!("</{name}>")),
        ("a", false) => {
            out.push_str("<a href=\"");
            push_attribute(out, &attribute(tag, "href").unwrap_or_default());
            out.push_str("\">");
        }
        ("img", false) => images.push(BodyImage {
            src: attribute(tag, "src").unwrap_or_default(),
            alt: attribute(tag, "alt").unwrap_or_default(),
        }),
        ("br", false) => out.push('\n'),
        // anything else is dropped, keeping the text around it
        _ => {}
    }
}

/// Reads an attribute value from a tag's contents, quoted or not.
fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut search_from = 0;

    while let Some(found) = lower[search_from..].find(name) {
        let start = search_from + found;
        search_from = start + name.len();

        // only match whole attribute names
        let preceded_by_space = lower[..start].ends_with(char::is_whitespace);
        let after = tag[start + name.len()..].trim_start();
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        if !preceded_by_space {
            continue;
        }

        let value = value.trim_start();
        return Some(
            match value.chars().next() {
                Some(quote @ ('"' | '\'')) => value[1..].split(quote).next().unwrap_or_default(),
                _ => value.split_whitespace().next().unwrap_or_default(),
            }
            .to_string(),
        );
    }

    None
}

/// Appends text, escaping markup characters but keeping valid entities.
fn push_text(out: &mut String, text: &str) {
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&escape_brackets(&rest[..amp]));
        let after = &rest[amp..];
        match entity_len(after) {
            Some(len) => {
                out.push_str(&after[..len]);
                rest = &after[len..];
            }
            None => {
                out.push_str("&amp;");
                rest = &after[1..];
            }
        }
    }
    out.push_str(&escape_brackets(rest));
}

/// Appends an attribute value, which additionally can't contain quotes.
fn push_attribute(out: &mut String, value: &str) {
    let mut escaped = String::new();
    push_text(&mut escaped, value);
    out.push_str(&escaped.replace('"', "&quot;"));
}

fn escape_brackets(text: &str) -> String {
    text.replace('<', "&lt;").replace('>', "&gt;")
}

/// The length of the entity at the start of `text` (which starts with `&`),
/// if it's a valid one.
fn entity_len(text: &str) -> Option<usize> {
    let end = text.find(';')?;
    let name = &text[1..end];
    let valid = matches!(name, "amp" | "lt" | "gt" | "quot" | "apos")
        || name
            .strip_prefix("#x")
            .is_some_and(|hex| !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()))
        || name
            .strip_prefix('#')
            .is_some_and(|dec| !dec.is_empty() && dec.chars().all(|c| c.is_ascii_digit()));
    valid.then_some(end + 1)
}

/// Checks markup with pango, which doesn't know gtk's `<a>` tags, so links
/// are checked as plain spans.
fn is_valid_markup(markup: &str) -> bool {
    let mut as_spans = String::with_capacity(markup.len());
    let mut rest = markup;
    while let Some(start) = rest.find("<a href=\"") {
        as_spans.push_str(&rest[..start]);
        as_spans.push_str("<span>");
        // sanitized link targets never contain a raw '>'
        rest = rest[start..].split_once('>').map_or("", |(_, after)| after);
    }
    as_spans.push_str(rest);

    pango::parse_markup(&as_spans.replace("</a>", "</span>"), '\0').is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_label_markup(body: &str) -> String {
        parse_body(body).markup
    }

    #[test]
    fn plain_text_is_escaped() {
        assert_eq!(to_label_markup("fish & chips"), "fish &amp; chips");
        assert_eq!(to_label_markup("a <3 b"), "a &lt;3 b");
    }

    #[test]
    fn valid_entities_are_kept() {
        assert_eq!(
            to_label_markup("1 &lt; 2 &amp;&#65;"),
            "1 &lt; 2 &amp;&#65;"
        );
    }

    #[test]
    fn supported_tags_are_kept() {
        assert_eq!(
            to_label_markup("<b>bold</b> <I>it</I> <u>under</u>"),
            "<b>bold</b> <i>it</i> <u>under</u>"
        );
    }

    #[test]
    fn unknown_tags_are_dropped() {
        assert_eq!(
            to_label_markup("<p>hi <font color=red>there</font></p>"),
            "hi there"
        );
    }

    #[test]
    fn links_keep_their_target() {
        assert_eq!(
            to_label_markup("<a href='https://example.com/?a=1&b=2'>site</a>"),
            "<a href=\"https://example.com/?a=1&amp;b=2\">site</a>"
        );
    }

    #[test]
    fn images_are_collected() {
        let body = parse_body("look <img src=\"/a.png\" alt=\"cat\"/>");
        assert_eq!(body.markup, "look ");
        assert_eq!(body.images, vec![BodyImage {
            src: "/a.png".to_string(),
            alt: "cat".to_string(),
        }]);
    }

    #[test]
    fn line_breaks_become_newlines() {
        assert_eq!(to_label_markup("a<br/>b"), "a\nb");
    }

    #[test]
    fn mismatched_tags_fall_back_to_plain_text() {
        assert_eq!(
            to_label_markup("<b><i>x</b></i>"),
            "&lt;b&gt;&lt;i&gt;x&lt;/b&gt;&lt;/i&gt;"
        );
    }
}
