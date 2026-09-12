use base64::{Engine, prelude::BASE64_STANDARD};
use image::{ImageFormat, ImageReader, Limits};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use uuid::Uuid;

pub const MAX_IMAGE_BYTES: usize = 3 * 1024 * 1024;
pub const MAX_BODY_BYTES: usize = 5 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FeedbackRequest {
    pub request_id: Uuid,
    pub kind: Kind,
    pub title: String,
    pub description: String,
    pub diagnostics_consent: bool,
    pub diagnostics: Option<Diagnostics>,
    pub screenshot: Option<Screenshot>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Bug,
    Feature,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostics {
    pub app_build: String,
    pub browser: String,
    pub os: String,
    pub viewport: String,
    pub language: String,
    pub theme: Theme,
    pub reduced_motion: bool,
    pub area: Option<Area>,
    pub error_code: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Light,
    Dark,
    System,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Area {
    Home,
    Learning,
    Courses,
    Challenges,
    Profile,
    Account,
    Settings,
    Shop,
    Events,
    Other,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Screenshot {
    pub data_url: String,
}

impl FeedbackRequest {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.request_id.is_nil() || self.request_id.get_version_num() != 4 {
            return Err("request_id must be a random version 4 UUID");
        }
        if self.title.trim().is_empty() || self.title.chars().count() > 256 {
            return Err("title must contain 1 to 256 characters");
        }
        if self.title.chars().any(char::is_control) {
            return Err("title must be one line");
        }
        if self.description.trim().is_empty() || self.description.chars().count() > 4096 {
            return Err("description must contain 1 to 4096 characters");
        }
        if self.diagnostics_consent != self.diagnostics.is_some() {
            return Err("diagnostics require explicit consent and the reviewed diagnostics object");
        }
        if let Some(d) = &self.diagnostics {
            for (value, max) in [
                (&d.app_build, 128),
                (&d.browser, 80),
                (&d.os, 80),
                (&d.viewport, 32),
                (&d.language, 32),
            ]
            .into_iter()
            .chain(d.error_code.as_ref().map(|s| (s, 64)))
            {
                if value.is_empty()
                    || value.len() > max
                    || !value
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._ -()×".contains(c))
                {
                    return Err("diagnostics must contain bounded neutral technical values");
                }
            }
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> String {
        // Struct serialization provides a stable field order independent of input JSON key order.
        format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(self).expect("serializable request"))
        )
    }

    pub fn issue_body(&self, marker: Uuid, image_url: Option<&str>) -> String {
        let kind = match self.kind {
            Kind::Bug => "Fehler",
            Kind::Feature => "Idee",
        };
        let mut body = format!("**Kategorie:** {kind}\n\n{}", self.description);
        if let Some(diagnostics) = &self.diagnostics {
            body.push_str("\n\n### Freiwillige technische Angaben\n\n```json\n");
            body.push_str(
                &serde_json::to_string_pretty(diagnostics).expect("serializable diagnostics"),
            );
            body.push_str("\n```");
        }
        if let Some(url) = image_url {
            body.push_str(&format!("\n\n### Freiwillig angehängter Screenshot\n\n![Screenshot]({url})\n\nDer Screenshot wird nach 90 Tagen von unserem Bildspeicher entfernt."));
        }
        body.push_str(&format!("\n\n{}", issue_marker(marker)));
        body
    }
}

pub fn issue_marker(marker: Uuid) -> String {
    format!("<!-- bootstrap-feedback:v1:{marker} -->")
}

/// Only final raster pixels leave this function. Metadata, source bytes and
/// annotation layers are never written to disk or supplied to GitHub.
pub fn sanitize_image(screenshot: &Screenshot) -> Result<Vec<u8>, &'static str> {
    let (format, encoded) =
        if let Some(s) = screenshot.data_url.strip_prefix("data:image/png;base64,") {
            (ImageFormat::Png, s)
        } else if let Some(s) = screenshot.data_url.strip_prefix("data:image/jpeg;base64,") {
            (ImageFormat::Jpeg, s)
        } else {
            return Err("screenshot must be a PNG or JPEG data URL");
        };
    if encoded.len() > MAX_IMAGE_BYTES.div_ceil(3) * 4 {
        return Err("screenshot exceeds 3 MiB");
    }
    let bytes = BASE64_STANDARD
        .decode(encoded)
        .map_err(|_| "invalid screenshot encoding")?;
    if bytes.len() > MAX_IMAGE_BYTES || image::guess_format(&bytes).ok() != Some(format) {
        return Err("invalid screenshot size or format");
    }
    let reader = ImageReader::with_format(Cursor::new(&bytes), format);
    let (width, height) = reader.into_dimensions().map_err(|_| "invalid screenshot")?;
    if width == 0
        || height == 0
        || width > 4096
        || height > 4096
        || u64::from(width) * u64::from(height) > 8 * 1024 * 1024
    {
        return Err("screenshot exceeds 4096 pixels per side or 8 megapixels");
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let mut pixels = reader
        .decode()
        .map_err(|_| "invalid screenshot")?
        .to_rgba8();
    // Flatten transparency so invisible RGB channels cannot preserve concealed
    // source pixels that a recipient could reveal by changing the alpha value.
    for pixel in pixels.pixels_mut() {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel.0[..3] {
            *channel = ((u32::from(*channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        }
        pixel[3] = 255;
    }
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(pixels)
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|_| "could not encode screenshot")?;
    let output = output.into_inner();
    if output.len() > MAX_IMAGE_BYTES {
        return Err("processed screenshot exceeds 3 MiB; crop or resize it");
    }
    Ok(output)
}
