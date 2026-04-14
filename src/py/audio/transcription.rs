use crate::engine::audio::{AudioListener, transcription::TranscriptionEngine};
use rustpython_vm::{AsObject, PyObjectRef, PyResult, VirtualMachine, function::FuncArgs};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

struct PyTranscriber {
    listener_ptr: usize,
    engine: TranscriptionEngine,
}

static ACTIVE_TRANSCRIBERS: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
static TRANSCRIBERS: OnceLock<Mutex<HashMap<usize, Box<PyTranscriber>>>> = OnceLock::new();

fn active_transcribers() -> &'static Mutex<HashSet<usize>> {
    ACTIVE_TRANSCRIBERS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn transcribers() -> &'static Mutex<HashMap<usize, Box<PyTranscriber>>> {
    TRANSCRIBERS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn transcription_new(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let mic_obj: PyObjectRef = if !args.args.is_empty() {
        args.args[0].clone()
    } else if let Some(obj) = args.kwargs.get("audio") {
        obj.clone()
    } else {
        return Err(vm.new_type_error(
            "xos.audio.transcription(audio, size='small|tiny') expects a microphone object"
                .to_string(),
        ));
    };
    let size: Option<String> = if args.args.len() > 1 {
        Some(args.args[1].clone().try_into_value::<String>(vm)?)
    } else if let Some(s) = args.kwargs.get("size") {
        Some(s.clone().try_into_value::<String>(vm)?)
    } else {
        None
    };
    if let Some(sz) = size.as_deref() {
        let lower = sz.trim().to_ascii_lowercase();
        if lower != "small" && lower != "tiny" {
            return Err(vm.new_value_error(
                "size must be 'small' or 'tiny'".to_string(),
            ));
        }
    }
    let listener_ptr_obj = mic_obj
        .get_attr("_listener_ptr", vm)
        .map_err(|_| vm.new_type_error("xos.audio.transcription expects xos.audio.Microphone".to_string()))?;
    let listener_ptr: usize = listener_ptr_obj.try_into_value(vm)?;
    if listener_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid microphone pointer".to_string()));
    }

    let mut engine = TranscriptionEngine::new_with_size(size.as_deref());
    let listener = unsafe { &*(listener_ptr as *const AudioListener) };
    engine.set_device_hint("python-mic", listener.buffer().sample_rate());

    let boxed = Box::new(PyTranscriber { listener_ptr, engine });
    let ptr = (&*boxed as *const PyTranscriber) as usize;
    if let Ok(mut map) = transcribers().lock() {
        map.insert(ptr, boxed);
    }
    if let Ok(mut set) = active_transcribers().lock() {
        set.insert(ptr);
    }

    let code = format!(
        r#"
class _Transcriber:
    def __init__(self, ptr):
        self._ptr = ptr

    def iterate(self, poll_interval=0.03):
        import xos
        while True:
            events = xos.audio._transcriber_next_events(self._ptr)
            if events:
                for ev in events:
                    yield ev
            else:
                xos.sleep(poll_interval)

    def __del__(self):
        if self._ptr != 0:
            import xos
            xos.audio._transcriber_cleanup(self._ptr)
            self._ptr = 0

_transcriber_instance = _Transcriber({})
"#,
        ptr
    );
    let scope = vm.new_scope_with_builtins();
    vm.run_code_string(scope.clone(), &code, "<transcriber>".to_string())?;
    scope.globals.get_item("_transcriber_instance", vm)
}

pub fn transcriber_next_events(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let ptr: usize = args.bind(vm)?;
    let mut map = transcribers()
        .lock()
        .map_err(|_| vm.new_runtime_error("transcriber lock poisoned".to_string()))?;
    let state = map
        .get_mut(&ptr)
        .ok_or_else(|| vm.new_runtime_error("Invalid transcriber pointer".to_string()))?;
    let listener = unsafe { &*(state.listener_ptr as *const AudioListener) };
    let channels = listener.get_samples_by_channel();
    let sr = listener.buffer().sample_rate();
    state.engine.process_snapshot(sr, &channels);
    let events = state.engine.drain_iter_events();
    let py_list = events
        .into_iter()
        .map(|e| match e {
            Some(line) => vm.ctx.new_str(line).into(),
            None => vm.ctx.none(),
        })
        .collect::<Vec<_>>();
    Ok(vm.ctx.new_list(py_list).into())
}

pub fn transcriber_cleanup(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let ptr: usize = args.bind(vm)?;
    if let Ok(mut set) = active_transcribers().lock() {
        set.remove(&ptr);
    }
    if let Ok(mut map) = transcribers().lock() {
        map.remove(&ptr);
    }
    Ok(vm.ctx.none())
}

