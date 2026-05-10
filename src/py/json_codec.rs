//! JSON bridging for RustPython: primitives, collections, and `builtins.Frame` (RGBA snapshot).
//!
//! Used by [`super::mesh`], [`super::mouse`], and [`super::json_api`] (`xos.json`).

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rustpython_vm::builtins::{PyBytes, PyDict, PyList, PyTuple, PyType};
use rustpython_vm::AsObject;
use rustpython_vm::{
    builtins::PyBaseExceptionRef, PyObjectRef, PyResult, VirtualMachine,
};
use serde_json::{json, Map, Number, Value};

const MAX_DEPTH: u32 = 48;

/// Wire key for encoded frames (nested inside a JSON object, e.g. `{"pic": {<this>}}`).
pub(crate) const XOS_JSON_FRAME: &str = "__xos_json_frame";

/// JPEG payload for mesh-serialized `Frame`: far smaller than RGBA base64 (~10×+ less wire + crypto).
const MESH_FRAME_JPEG_QUALITY: u8 = 78;

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

fn frame_rgba_to_wire_json(w: usize, h: usize, rgba: &[u8]) -> Value {
    rgba_to_jpeg_xos_wire(w, h, rgba).unwrap_or_else(|_| {
        let b64 = B64.encode(rgba);
        json!({
            XOS_JSON_FRAME: { "w": w, "h": h, "rgba_b64": b64 }
        })
    })
}

#[inline]
fn type_error(vm: &VirtualMachine, msg: impl Into<String>) -> PyBaseExceptionRef {
    vm.new_type_error(msg.into())
}

/// Build `Frame(width, tensor with uint8 RGBA PyBytes)` from raw pixels.
pub(crate) fn py_frame_from_rgba_bytes(
    vm: &VirtualMachine,
    width: usize,
    height: usize,
    rgba: Vec<u8>,
) -> PyResult {
    let need = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| vm.new_value_error("Frame: width×height overflow".to_string()))?;
    if rgba.len() != need {
        return Err(vm.new_value_error(format!(
            "Frame: rgba length {} ≠ {}×{}×4",
            rgba.len(),
            width,
            height
        )));
    }

    let py_bytes = vm.ctx.new_bytes(rgba);
    let tensor_dict = vm.ctx.new_dict();
    tensor_dict.set_item(
        "shape",
        vm.ctx
            .new_tuple(vec![
                vm.ctx.new_int(height as isize).into(),
                vm.ctx.new_int(width as isize).into(),
                vm.ctx.new_int(4_isize).into(),
            ])
            .into(),
        vm,
    )?;
    tensor_dict.set_item("dtype", vm.ctx.new_str("uint8").into(), vm)?;
    tensor_dict.set_item("device", vm.ctx.new_str("cpu").into(), vm)?;
    tensor_dict.set_item("_data", py_bytes.into(), vm)?;

    let frame_dict = vm.ctx.new_dict();
    frame_dict.set_item("width", vm.ctx.new_int(width as isize).into(), vm)?;
    frame_dict.set_item("height", vm.ctx.new_int(height as isize).into(), vm)?;
    frame_dict.set_item("tensor", tensor_dict.into(), vm)?;

    let frame_cls = vm.builtins.get_attr("Frame", vm)?;
    frame_cls.call((frame_dict,), vm)
}

fn is_builtin_frame(vm: &VirtualMachine, obj: &PyObjectRef) -> PyResult<bool> {
    let Ok(cls_obj) = vm.builtins.get_attr("Frame", vm) else {
        return Ok(false);
    };
    let Some(cls) = cls_obj.downcast_ref::<PyType>() else {
        return Ok(false);
    };
    Ok(obj.fast_isinstance(cls))
}

/// Snapshot framebuffer pixels into owned RGBA (see mesh / rasterizer docs).
pub(crate) fn frame_rgba_to_json_value(vm: &VirtualMachine, obj: &PyObjectRef) -> PyResult<Value> {
    let Some(inner) = vm.get_attribute_opt(obj.clone(), "_data")? else {
        return Err(type_error(vm, "Frame missing _data"));
    };
    let frame_dict = inner
        .downcast_ref::<PyDict>()
        .ok_or_else(|| type_error(vm, "Frame._data must be a dict"))?;

    let w: usize = frame_dict
        .get_item("width", vm)?
        .clone()
        .try_into_value::<i64>(vm)? as usize;
    let h: usize = frame_dict
        .get_item("height", vm)?
        .clone()
        .try_into_value::<i64>(vm)? as usize;
    let need = w
        .checked_mul(h)
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(|| vm.new_value_error("Frame: width×height overflow".to_string()))?;

    let tensor_any = frame_dict.get_item("tensor", vm)?;
    let tensor_dict = tensor_any
        .downcast_ref::<PyDict>()
        .ok_or_else(|| type_error(vm, "Frame.tensor must be a dict"))?;

    // 1) Existing materialized `_data` (bytes)
    if let Ok(blob) = tensor_dict.get_item("_data", vm) {
        if let Some(bytes) = blob.downcast_ref::<PyBytes>() {
            let s = bytes.as_bytes();
            if s.len() == need {
                return Ok(frame_rgba_to_wire_json(w, h, s));
            }
        }
    }

    // 2) Standalone engine buffer keyed by `_xos_viewport_id`
    if let Ok(vid_obj) = tensor_dict.get_item("_xos_viewport_id", vm) {
        if let Ok(vid) = vid_obj.clone().try_into_value::<i64>(vm) {
            if let Some(buf) =
                crate::python_api::xos_module::standalone_frame_buffer_copy(vid.max(0) as u64)
            {
                if buf.len() == need {
                    return Ok(frame_rgba_to_wire_json(w, h, &buf));
                }
            }
        }
    }

    // 3) Active raster tick buffer (dimensions must match this Frame)
    if let Some(buf) = crate::python_api::rasterizer::copy_active_frame_rgba_if_match(w, h) {
        if buf.len() == need {
            return Ok(frame_rgba_to_wire_json(w, h, &buf));
        }
    }

    // 4) Legacy list-backed `tensor["data"]`
    if let Ok(data_obj) = tensor_dict.get_item("data", vm) {
        if let Some(lst) = data_obj.downcast_ref::<PyList>() {
            let items = lst.borrow_vec();
            if items.len() == need {
                let mut raw = Vec::with_capacity(need);
                for item in items.iter() {
                    let v: i32 = item.clone().try_into_value(vm)?;
                    raw.push(v.clamp(0, 255) as u8);
                }
                return Ok(frame_rgba_to_wire_json(w, h, &raw));
            }
        }
    }

    Err(vm.new_runtime_error(
        "cannot serialize Frame: need RGBA (tensor._data bytes), standalone buffer, matching active framebuffer, or tensor data list".into(),
    ))
}

pub(crate) fn try_decode_xos_json_frame_object(
    vm: &VirtualMachine,
    map: &Map<String, Value>,
) -> Option<PyResult> {
    if map.len() != 1 {
        return None;
    }
    let body = map.get(XOS_JSON_FRAME)?;
    let b = body.as_object()?;
    if let Some(jpeg_s) = b
        .get("jpeg_b64")
        .and_then(|x| x.as_str())
        .or_else(|| b.get("jpeg").and_then(|x| x.as_str()))
    {
        let raw_jpeg = match B64.decode(jpeg_s.as_bytes()) {
            Ok(bytes) => bytes,
            Err(e) => {
                return Some(Err(vm.new_runtime_error(format!(
                    "{XOS_JSON_FRAME}: invalid jpeg base64 ({e})"
                ))));
            }
        };
        let img = match image::load_from_memory(&raw_jpeg) {
            Ok(img) => img,
            Err(e) => {
                return Some(Err(vm.new_runtime_error(format!(
                    "{XOS_JSON_FRAME}: decode jpeg: {e}"
                ))));
            }
        };
        let rgba = img.to_rgba8();
        let w = rgba.width() as usize;
        let h = rgba.height() as usize;
        return Some(py_frame_from_rgba_bytes(vm, w, h, rgba.into_raw()));
    }
    let w = b.get("w").and_then(|x| x.as_u64()).or_else(|| b.get("width").and_then(|x| x.as_u64()))?
        as usize;
    let h = b.get("h").and_then(|x| x.as_u64()).or_else(|| b.get("height").and_then(|x| x.as_u64()))?
        as usize;
    let enc =
        b.get("rgba_b64")
            .and_then(|x| x.as_str())
            .or_else(|| b.get("b64").and_then(|x| x.as_str()))?;
    let raw = match B64.decode(enc.as_bytes()) {
        Ok(bytes) => bytes,
        Err(e) => {
            return Some(Err(vm.new_runtime_error(format!(
                "{XOS_JSON_FRAME}: invalid base64 ({e})"
            ))));
        }
    };
    Some(py_frame_from_rgba_bytes(vm, w, h, raw))
}

/// Python value → `serde_json::Value` (mesh transport, file IO, IPC).
pub(crate) fn py_to_json_value(
    vm: &VirtualMachine,
    obj: PyObjectRef,
    depth: u32,
) -> Result<Value, PyBaseExceptionRef> {
    if depth > MAX_DEPTH {
        return Err(vm.new_value_error(format!(
            "JSON encode: nesting deeper than {} levels",
            MAX_DEPTH
        )));
    }
    if vm.is_none(&obj) {
        return Ok(Value::Null);
    }
    if let Ok(b) = obj.clone().try_into_value::<bool>(vm) {
        return Ok(Value::Bool(b));
    }
    if let Ok(i) = obj.clone().try_into_value::<i64>(vm) {
        return Ok(Value::Number(i.into()));
    }
    if let Ok(f) = obj.clone().try_into_value::<f64>(vm) {
        return Ok(Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null));
    }
    if let Ok(s) = obj.clone().try_into_value::<String>(vm) {
        return Ok(Value::String(s));
    }

    if is_builtin_frame(vm, &obj)? {
        return frame_rgba_to_json_value(vm, &obj);
    }

    if let Some(list) = obj.downcast_ref::<PyList>() {
        let mut arr = Vec::with_capacity(list.borrow_vec().len());
        for item in list.borrow_vec().iter() {
            arr.push(py_to_json_value(vm, item.clone(), depth + 1)?);
        }
        return Ok(Value::Array(arr));
    }
    if let Some(tup) = obj.downcast_ref::<PyTuple>() {
        let mut arr = Vec::with_capacity(tup.as_slice().len());
        for item in tup.as_slice().iter() {
            arr.push(py_to_json_value(vm, item.clone(), depth + 1)?);
        }
        return Ok(Value::Array(arr));
    }
    if let Some(dict) = obj.downcast_ref::<PyDict>() {
        let mut map = serde_json::Map::new();
        for (key, val) in dict {
            let key_str = key.str(vm)?.to_string();
            map.insert(key_str, py_to_json_value(vm, val.clone(), depth + 1)?);
        }
        return Ok(Value::Object(map));
    }

    Err(type_error(vm, "object is not JSON-serializable (use None, bool, int, float, str, list, tuple, dict, or builtins.Frame — or implement your own envelope)"))
}

/// JSON value → Python (`Frame` reconstructed from [`XOS_JSON_FRAME`] blobs).
pub(crate) fn json_value_to_py(vm: &VirtualMachine, v: &Value) -> PyResult {
    match v {
        Value::Null => Ok(vm.ctx.none()),
        Value::Bool(b) => Ok(vm.ctx.new_bool(*b).into()),
        Value::Number(n) => Ok(if let Some(i) = n.as_i64() {
            vm.ctx.new_int(i).into()
        } else if let Some(f) = n.as_f64() {
            vm.ctx.new_float(f).into()
        } else {
            vm.ctx.new_str(n.to_string()).into()
        }),
        Value::String(s) => Ok(vm.ctx.new_str(s.as_str()).into()),
        Value::Array(a) => {
            let mut items = Vec::with_capacity(a.len());
            for x in a {
                items.push(json_value_to_py(vm, x)?);
            }
            Ok(vm.ctx.new_list(items).into())
        }
        Value::Object(o) => {
            if let Some(r) = try_decode_xos_json_frame_object(vm, o) {
                return r;
            }
            let d = vm.ctx.new_dict();
            for (k, val) in o {
                d.set_item(k.as_str(), json_value_to_py(vm, val)?, vm)?;
            }
            Ok(d.into())
        }
    }
}
