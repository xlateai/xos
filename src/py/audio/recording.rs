//! MP3 capture from an existing `xos.audio.Microphone` / `AudioListener` (native desktop only).

use crate::engine::audio::AudioListener;
use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, MonoPcm, Quality};
use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine, function::FuncArgs};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

struct PyRecorder {
    listener_ptr: usize,
    path: PathBuf,
    encoder: Mutex<Option<RecorderMp3>>,
}

/// LAME expects PCM in multiples of 1152 samples per channel for each `encode` call.
struct RecorderMp3 {
    enc: mp3lame_encoder::Encoder,
    file: File,
    scratch_mp3: Vec<u8>,
    stereo: bool,
    pending_l: Vec<i16>,
    pending_r: Vec<i16>,
}

static RECORDERS: OnceLock<Mutex<HashMap<usize, Box<PyRecorder>>>> = OnceLock::new();

fn recorders() -> &'static Mutex<HashMap<usize, Box<PyRecorder>>> {
    RECORDERS.get_or_init(|| Mutex::new(HashMap::new()))
}

const PCM_CHUNK: usize = 1152;

fn f32_to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16
}

fn ensure_mp3_encoder(sr: u32, ch: u16) -> Result<mp3lame_encoder::Encoder, String> {
    let mut b = Builder::new().ok_or_else(|| "LAME encoder builder unavailable".to_string())?;
    b.set_sample_rate(sr)
        .map_err(|e| format!("sample rate {sr} Hz not supported by MP3 encoder: {e}"))?;
    b.set_num_channels(ch as u8)
        .map_err(|e| format!("set channels: {e}"))?;
    b.set_brate(Bitrate::Kbps128)
        .map_err(|e| format!("set bitrate: {e}"))?;
    b.set_quality(Quality::Best)
        .map_err(|e| format!("set quality: {e}"))?;
    b.build().map_err(|e| format!("LAME init: {e}"))
}

fn encode_pcm_chunk(
    enc: &mut mp3lame_encoder::Encoder,
    scratch_mp3: &mut Vec<u8>,
    file: &mut File,
    left: &[i16],
    right: Option<&[i16]>,
) -> Result<(), String> {
    debug_assert_eq!(left.len(), PCM_CHUNK);
    if let Some(r) = right {
        debug_assert_eq!(r.len(), PCM_CHUNK);
    }
    scratch_mp3.clear();
    scratch_mp3.reserve(mp3lame_encoder::max_required_buffer_size(left.len()));
    let n = match right {
        Some(r) => {
            let input = DualPcm { left, right: r };
            enc.encode(input, scratch_mp3.spare_capacity_mut())
                .map_err(|e| format!("encode: {e}"))?
        }
        _ => {
            let input = MonoPcm(left);
            enc.encode(input, scratch_mp3.spare_capacity_mut())
                .map_err(|e| format!("encode: {e}"))?
        }
    };
    unsafe {
        scratch_mp3.set_len(n);
    }
    file.write_all(scratch_mp3)
        .map_err(|e| format!("write mp3: {e}"))?;
    Ok(())
}

impl RecorderMp3 {
    fn append_samples(&mut self, left: &[i16], right: Option<&[i16]>) -> Result<(), String> {
        self.pending_l.extend_from_slice(left);
        if self.stereo {
            let r = right.ok_or("internal error: stereo without right channel")?;
            self.pending_r.extend_from_slice(r);
        }
        self.encode_pending_full_frames()
    }

    fn encode_pending_full_frames(&mut self) -> Result<(), String> {
        while self.pending_l.len() >= PCM_CHUNK {
            let l = &self.pending_l[..PCM_CHUNK];
            let r = if self.stereo {
                Some(&self.pending_r[..PCM_CHUNK])
            } else {
                None
            };
            encode_pcm_chunk(
                &mut self.enc,
                &mut self.scratch_mp3,
                &mut self.file,
                l,
                r,
            )?;
            self.pending_l.drain(..PCM_CHUNK);
            if self.stereo {
                self.pending_r.drain(..PCM_CHUNK);
            }
        }
        Ok(())
    }

    /// Zero-pad the last partial frame, encode it, then flush the encoder.
    fn finalize(&mut self) -> Result<(), String> {
        self.encode_pending_full_frames()?;
        if !self.pending_l.is_empty() {
            self.pending_l.resize(PCM_CHUNK, 0);
            if self.stereo {
                self.pending_r.resize(PCM_CHUNK, 0);
            }
            let r = if self.stereo {
                Some(self.pending_r.as_slice())
            } else {
                None
            };
            encode_pcm_chunk(
                &mut self.enc,
                &mut self.scratch_mp3,
                &mut self.file,
                self.pending_l.as_slice(),
                r,
            )?;
            self.pending_l.clear();
            self.pending_r.clear();
        }

        self.scratch_mp3.clear();
        self.scratch_mp3
            .reserve(mp3lame_encoder::max_required_buffer_size(PCM_CHUNK * 2));
        let flush_n = self
            .enc
            .flush::<FlushNoGap>(self.scratch_mp3.spare_capacity_mut())
            .map_err(|e| format!("flush mp3: {e}"))?;
        unsafe {
            self.scratch_mp3.set_len(flush_n);
        }
        self.file
            .write_all(&self.scratch_mp3)
            .map_err(|e| format!("write final mp3: {e}"))?;
        self.file.sync_all().map_err(|e| format!("sync: {e}"))?;
        Ok(())
    }
}

pub fn recording_new(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let mic_obj: PyObjectRef = if !args.args.is_empty() {
        args.args[0].clone()
    } else if let Some(obj) = args.kwargs.get("audio") {
        obj.clone()
    } else {
        return Err(vm.new_type_error(
            "xos.audio.recording(mic, path) or recording(audio=mic, path=\"file.mp3\")".to_string(),
        ));
    };
    let path_str: String = if args.args.len() > 1 {
        args.args[1].clone().try_into_value(vm)?
    } else if let Some(p) = args.kwargs.get("path") {
        p.clone().try_into_value(vm)?
    } else {
        return Err(vm.new_type_error("recording: missing path".to_string()));
    };

    let path = PathBuf::from(path_str.trim());
    if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("mp3")) != Some(true) {
        return Err(vm.new_value_error("path must end with .mp3".to_string()));
    }

    let listener_ptr_obj = mic_obj
        .get_attr("_listener_ptr", vm)
        .map_err(|_| vm.new_type_error("xos.audio.recording expects xos.audio.Microphone".to_string()))?;
    let listener_ptr: usize = listener_ptr_obj.try_into_value(vm)?;
    if listener_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid microphone pointer".to_string()));
    }

    let boxed = Box::new(PyRecorder {
        listener_ptr,
        path,
        encoder: Mutex::new(None),
    });
    let ptr = (&*boxed as *const PyRecorder) as usize;
    if let Ok(mut map) = recorders().lock() {
        map.insert(ptr, boxed);
    }

    let code = format!(
        r#"
class Recording:
    def __init__(self, ptr):
        self._ptr = ptr

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        self.finish()
        return False

    def record(self, poll_interval=0.02):
        import xos
        while True:
            xos.audio._recording_step(self._ptr)
            xos.sleep(poll_interval)

    def finish(self):
        if self._ptr != 0:
            import xos
            xos.audio._recording_finish(self._ptr)
            self._ptr = 0

    def __del__(self):
        self.finish()

_recording_instance = Recording({})
"#,
        ptr
    );
    let scope = vm.new_scope_with_builtins();
    vm.run_code_string(scope.clone(), &code, "<recording>".to_string())?;
    scope.globals.get_item("_recording_instance", vm)
}

pub fn recording_step(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let ptr: usize = args.bind(vm)?;
    let mut map = recorders()
        .lock()
        .map_err(|_| vm.new_runtime_error("recording lock poisoned".to_string()))?;
    let rec = map
        .get_mut(&ptr)
        .ok_or_else(|| vm.new_runtime_error("Invalid recording pointer".to_string()))?;

    let listener = unsafe { &*(rec.listener_ptr as *const AudioListener) };
    let buf = listener.buffer();
    let n = buf.len();
    if n == 0 {
        return Ok(vm.ctx.none());
    }

    let drained = buf.drain_samples(n);
    if drained.is_empty() || drained[0].is_empty() {
        return Ok(vm.ctx.none());
    }

    let sr = buf.sample_rate();
    let ch = buf.channels();
    if ch == 0 || ch > 2 {
        return Err(vm.new_runtime_error(format!(
            "MP3 recording supports 1–2 channels; this device reports {ch}"
        )));
    }

    let mut guard = rec
        .encoder
        .lock()
        .map_err(|_| vm.new_runtime_error("recording encoder lock poisoned".to_string()))?;

    if guard.is_none() {
        let enc = ensure_mp3_encoder(sr, ch).map_err(|e| vm.new_runtime_error(e))?;
        let file = File::create(&rec.path).map_err(|e| vm.new_runtime_error(format!("create {}: {e}", rec.path.display())))?;
        *guard = Some(RecorderMp3 {
            enc,
            file,
            scratch_mp3: Vec::with_capacity(8192),
            stereo: ch == 2,
            pending_l: Vec::new(),
            pending_r: Vec::new(),
        });
    }

    let inner = guard.as_mut().expect("just set");
    let frames = drained[0].len();
    let left_i16: Vec<i16> = drained[0].iter().copied().map(f32_to_i16).collect();
    let right_i16: Option<Vec<i16>> = if ch == 2 {
        if drained.len() < 2 || drained[1].len() != frames {
            return Err(vm.new_runtime_error("stereo channel length mismatch".to_string()));
        }
        Some(drained[1].iter().copied().map(f32_to_i16).collect())
    } else {
        None
    };

    let right_slice = right_i16.as_ref().map(|v| v.as_slice());
    inner
        .append_samples(left_i16.as_slice(), right_slice)
        .map_err(|e| vm.new_runtime_error(e))?;

    Ok(vm.ctx.none())
}

pub fn recording_finish(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let ptr: usize = args.bind(vm)?;
    let mut map = recorders()
        .lock()
        .map_err(|_| vm.new_runtime_error("recording lock poisoned".to_string()))?;
    let rec = map
        .remove(&ptr)
        .ok_or_else(|| vm.new_runtime_error("Invalid recording pointer".to_string()))?;

    let mut guard = rec
        .encoder
        .lock()
        .map_err(|_| vm.new_runtime_error("recording encoder lock poisoned".to_string()))?;
    if let Some(mut inner) = guard.take() {
        inner
            .finalize()
            .map_err(|e| vm.new_runtime_error(e))?;
    }

    Ok(vm.ctx.none())
}

/// Best-effort finalize open MP3 files (e.g. process exit); ignores errors.
pub fn cleanup_all_recordings_rust() {
    let Ok(mut map) = RECORDERS.get_or_init(|| Mutex::new(HashMap::new())).lock() else {
        return;
    };
    for (_, rec) in map.drain() {
        let Ok(mut guard) = rec.encoder.lock() else {
            continue;
        };
        if let Some(mut inner) = guard.take() {
            let _ = inner.finalize();
        }
    }
}
