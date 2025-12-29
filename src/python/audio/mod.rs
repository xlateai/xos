use rustpython_vm::{VirtualMachine, builtins::PyModule, PyRef};

mod microphone;
mod speakers;

// Re-export cleanup functions for use by other modules
pub use microphone::cleanup_all_microphones_rust;
pub use speakers::cleanup_all_speakers_rust;

/// Create the audio module with both microphone and speaker support
pub fn make_audio_module(vm: &VirtualMachine) -> PyRef<PyModule> {
    let module = vm.new_module("xos.audio", vm.ctx.new_dict(), None);
    
    // --- Microphone API ---
    module.set_attr("get_input_devices", vm.new_function("get_input_devices", microphone::get_input_devices), vm).unwrap();
    module.set_attr("Microphone", vm.new_function("Microphone", microphone::microphone_new), vm).unwrap();
    module.set_attr("cleanup_all_microphones", vm.new_function("cleanup_all_microphones", microphone::cleanup_all_microphones), vm).unwrap();
    
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
    
    // --- Speaker API ---
    module.set_attr("get_output_devices", vm.new_function("get_output_devices", speakers::get_output_devices), vm).unwrap();
    module.set_attr("Speaker", vm.new_function("Speaker", speakers::speaker_new), vm).unwrap();
    module.set_attr("cleanup_all_speakers", vm.new_function("cleanup_all_speakers", speakers::cleanup_all_speakers), vm).unwrap();
    
    // Internal speaker functions
    module.set_attr("_speaker_play_batch", vm.new_function("_speaker_play_batch", speakers::speaker_play_batch), vm).unwrap();
    module.set_attr("_speaker_get_buffer", vm.new_function("_speaker_get_buffer", speakers::speaker_get_buffer), vm).unwrap();
    module.set_attr("_speaker_cleanup", vm.new_function("_speaker_cleanup", speakers::speaker_cleanup), vm).unwrap();
    
    module
}

/// Clean up all audio resources (both microphones and speakers)
pub fn cleanup_all_audio() {
    microphone::cleanup_all_microphones_rust();
    speakers::cleanup_all_speakers_rust();
}

