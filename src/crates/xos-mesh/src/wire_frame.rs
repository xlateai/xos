//! RGBA → mesh wire JSON (`{"frame": …}`) without RustPython (used by relay + Python codec).

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Value};

pub const XOS_JSON_FRAME: &str = "__xos_json_frame";

const MESH_FRAME_JPEG_QUALITY: u8 = 56;

#[inline]
fn rgba_to_jpeg_xos_wire(w: usize, h: usize, rgba: &[u8]) -> Result<Value, ()> {
    let w_u = u32::try_from(w).map_err(|_| ())?;
    let h_u = u32::try_from(h).map_err(|_| ())?;
    let Some(image_rgba) = image::RgbaImage::from_raw(w_u, h_u, rgba.to_vec()) else {
        return Err(());
    };
    let source = image::DynamicImage::ImageRgba8(image_rgba);
    let mut jpeg_bytes = Vec::new();
    {
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(
            &mut jpeg_bytes,
            MESH_FRAME_JPEG_QUALITY,
        );
        enc.encode_image(&source).map_err(|_| ())?;
    }
    let b64 = B64.encode(&jpeg_bytes);
    Ok(json!({
        XOS_JSON_FRAME: { "w": w, "h": h, "jpeg_b64": b64 }
    }))
}

pub fn frame_rgba_to_mesh_wire_value(w: usize, h: usize, rgba: &[u8]) -> Value {
    rgba_to_jpeg_xos_wire(w, h, rgba).unwrap_or_else(|_| {
        let b64 = B64.encode(rgba);
        json!({
            XOS_JSON_FRAME: { "w": w, "h": h, "rgba_b64": b64 }
        })
    })
}

/// Wire body `{"frame": …}` built from RGBA slice (caller runs off the interpreter thread).
pub fn mesh_broadcast_body_from_rgba(w: u32, h: u32, rgba: &[u8]) -> Value {
    let inner = frame_rgba_to_mesh_wire_value(w as usize, h as usize, rgba);
    json!({ "frame": inner })
}
