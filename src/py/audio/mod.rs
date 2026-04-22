use rustpython_vm::{VirtualMachine, builtins::PyModule, PyRef};
use rustpython_vm::{PyResult, function::FuncArgs};

mod microphone;
mod speakers;
mod transcription;
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
mod recording;

// Re-export cleanup functions for use by other modules
pub use microphone::cleanup_all_microphones_rust;
pub use speakers::cleanup_all_speakers_rust;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
fn wrap_tensor_dict(dict: rustpython_vm::PyObjectRef, vm: &VirtualMachine) -> PyResult {
    if let Ok(wrapper_class) = vm.builtins.get_attr("Tensor", vm) {
        if let Ok(wrapped) = wrapper_class.call((dict.clone(),), vm) {
            return Ok(wrapped);
        }
    }
    Ok(dict)
}

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
fn resample_linear(input: &[f32], src_rate: u32, dst_rate: u32) -> Vec<f32> {
    if input.is_empty() || src_rate == 0 || dst_rate == 0 || src_rate == dst_rate {
        return input.to_vec();
    }
    let out_len = ((input.len() as u64) * (dst_rate as u64) / (src_rate as u64)).max(1) as usize;
    let mut out = Vec::with_capacity(out_len);
    let scale = src_rate as f32 / dst_rate as f32;
    for i in 0..out_len {
        let pos = i as f32 * scale;
        let idx = pos.floor() as usize;
        let frac = pos - idx as f32;
        let a = input.get(idx).copied().unwrap_or(0.0);
        let b = input.get(idx + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
}

/// Load audio to mono **f32** PCM, default **16_000 Hz** — the rate Whisper / in-tree `whisper_burn`
/// expect for `transcribe(..., sample_rate, ...)`. Samples are roughly **[-1, 1]** after decode.
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
fn audio_load(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    use std::path::Path;
    use crate::engine::audio::decode_path_to_mono_f32;
    use crate::python_api::dtypes::DType;
    use crate::python_api::tensors::{Tensor, create_tensor_from_data};

    let path: String = if let Some(v) = args.args.first() {
        v.clone().try_into_value(vm)?
    } else {
        return Err(vm.new_type_error(
            "xos.audio.load(path, sample_rate=16000) requires path".to_string(),
        ));
    };
    let target_sample_rate: i64 = if let Some(v) = args.args.get(1) {
        v.clone().try_into_value(vm)?
    } else {
        16_000
    };
    if target_sample_rate <= 0 {
        return Err(vm.new_value_error("sample_rate must be > 0".to_string()));
    }

    let (src_rate, _duration, mono) = decode_path_to_mono_f32(Path::new(&path))
        .map_err(|e| vm.new_runtime_error(format!("decode audio file: {e}")))?;

    let mono = resample_linear(&mono, src_rate, target_sample_rate as u32);
    let shape = vec![mono.len()];
    let py_tensor: Tensor = create_tensor_from_data(mono, shape, DType::Float32);
    wrap_tensor_dict(py_tensor.to_py_dict(vm, DType::Float32)?, vm)
}

#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
fn audio_load(_args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    Err(vm.new_runtime_error(
        "xos.audio.load is only available on desktop (macOS/Linux/Windows), not iOS/WASM"
            .to_string(),
    ))
}

#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
fn recording_stub_new(
    _args: rustpython_vm::function::FuncArgs,
    vm: &VirtualMachine,
) -> rustpython_vm::PyResult {
    Err(vm.new_runtime_error(
        "xos.audio.recording is only available on desktop (macOS/Linux/Windows), not iOS/WASM".to_string(),
    ))
}

/// Create the audio module with both microphone and speaker support
pub fn make_audio_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.audio", vm.ctx.new_dict(), None);
    
    // --- Microphone API ---
    module.set_attr("get_input_devices", vm.new_function("get_input_devices", microphone::get_input_devices), vm).unwrap();
    module.set_attr("Microphone", vm.new_function("Microphone", microphone::microphone_new), vm).unwrap();
    module.set_attr("system", vm.new_function("system", microphone::microphone_system), vm).unwrap();
    module
        .set_attr("load", vm.new_function("load", audio_load), vm)
        .unwrap();
    module.set_attr("cleanup_all_microphones", vm.new_function("cleanup_all_microphones", microphone::cleanup_all_microphones), vm).unwrap();
    module.set_attr("transcription", vm.new_function("transcription", transcription::transcription_new), vm).unwrap();
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    {
        module
            .set_attr("recording", vm.new_function("recording", recording::recording_new), vm)
            .unwrap();
        module
            .set_attr(
                "_recording_step",
                vm.new_function("_recording_step", recording::recording_step),
                vm,
            )
            .unwrap();
        module
            .set_attr(
                "_recording_finish",
                vm.new_function("_recording_finish", recording::recording_finish),
                vm,
            )
            .unwrap();
    }
    #[cfg(any(target_arch = "wasm32", target_os = "ios"))]
    {
        module
            .set_attr("recording", vm.new_function("recording", recording_stub_new), vm)
            .unwrap();
    }
    
    // Internal microphone functions
    module.set_attr("_microphone_get_batch", vm.new_function("_microphone_get_batch", microphone::microphone_get_batch), vm).unwrap();
    module.set_attr("_microphone_get_all", vm.new_function("_microphone_get_all", microphone::microphone_get_all), vm).unwrap();
    module.set_attr("_microphone_read_batch", vm.new_function("_microphone_read_batch", microphone::microphone_read_batch), vm).unwrap();
    module.set_attr("_microphone_read_all", vm.new_function("_microphone_read_all", microphone::microphone_read_all), vm).unwrap();
    module.set_attr("_microphone_clear", vm.new_function("_microphone_clear", microphone::microphone_clear), vm).unwrap();
    module.set_attr("_microphone_pause", vm.new_function("_microphone_pause", microphone::microphone_pause), vm).unwrap();
    module.set_attr("_microphone_record", vm.new_function("_microphone_record", microphone::microphone_record), vm).unwrap();
    module.set_attr("_microphone_get_sample_rate", vm.new_function("_microphone_get_sample_rate", microphone::microphone_get_sample_rate), vm).unwrap();
    module.set_attr("_microphone_cleanup", vm.new_function("_microphone_cleanup", microphone::microphone_cleanup), vm).unwrap();
    module.set_attr("_transcriber_next_events", vm.new_function("_transcriber_next_events", transcription::transcriber_next_events), vm).unwrap();
    module.set_attr(
        "_transcriber_transcribe_step",
        vm.new_function("_transcriber_transcribe_step", transcription::transcriber_transcribe_step),
        vm,
    )
    .unwrap();
    module
        .set_attr(
            "_transcriber_vad_prob",
            vm.new_function("_transcriber_vad_prob", transcription::transcriber_vad_prob),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_transcriber_buffered_seconds",
            vm.new_function(
                "_transcriber_buffered_seconds",
                transcription::transcriber_buffered_seconds,
            ),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_transcriber_clip_cursor",
            vm.new_function("_transcriber_clip_cursor", transcription::transcriber_clip_cursor),
            vm,
        )
        .unwrap();
    module
        .set_attr(
            "_transcriber_flush_commit",
            vm.new_function("_transcriber_flush_commit", transcription::transcriber_flush_commit),
            vm,
        )
        .unwrap();
    module.set_attr("_transcriber_cleanup", vm.new_function("_transcriber_cleanup", transcription::transcriber_cleanup), vm).unwrap();
    
    // --- Speaker API ---
    module.set_attr("get_output_devices", vm.new_function("get_output_devices", speakers::get_output_devices), vm).unwrap();
    module.set_attr("Speaker", vm.new_function("Speaker", speakers::speaker_new), vm).unwrap();
    module.set_attr("cleanup_all_speakers", vm.new_function("cleanup_all_speakers", speakers::cleanup_all_speakers), vm).unwrap();
    
    // Internal speaker functions
    module.set_attr("_speaker_play_batch", vm.new_function("_speaker_play_batch", speakers::speaker_play_batch), vm).unwrap();
    module.set_attr("_speaker_get_buffer_size", vm.new_function("_speaker_get_buffer_size", speakers::speaker_get_buffer_size), vm).unwrap();
    module.set_attr("_speaker_get_buffer", vm.new_function("_speaker_get_buffer", speakers::speaker_get_buffer), vm).unwrap();
    module.set_attr("_speaker_cleanup", vm.new_function("_speaker_cleanup", speakers::speaker_cleanup), vm).unwrap();

    module
}

/// Clean up all audio resources (both microphones and speakers)
pub fn cleanup_all_audio() {
    microphone::cleanup_all_microphones_rust();
    speakers::cleanup_all_speakers_rust();
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "ios")))]
    recording::cleanup_all_recordings_rust();
}

