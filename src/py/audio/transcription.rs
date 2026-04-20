use crate::ai::transcription::{TranscriptionEngine, WhisperBackend};
use crate::engine::audio::AudioListener;
use rustpython_vm::{PyObjectRef, PyResult, VirtualMachine, function::FuncArgs};
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

struct PyTranscriber {
    listener_ptr: usize,
    engine: TranscriptionEngine,
    /// Last [`TranscriptionEngine::transcript_epoch`] seen from [`transcriber_transcribe_step`].
    last_yield_epoch: u64,
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
            "xos.audio.transcription(audio, size='tiny|small|base', backend='ct2|burn', language='english|japanese') expects a microphone object"
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
        if lower != "small" && lower != "tiny" && lower != "base" {
            return Err(vm.new_value_error(
                "size must be 'small', 'tiny', or 'base'".to_string(),
            ));
        }
    }
    let backend_kw = args.kwargs.get("backend").cloned();
    let backend_s: Option<String> = if let Some(b) = backend_kw {
        Some(b.try_into_value::<String>(vm)?)
    } else {
        None
    };
    let backend = match backend_s.as_deref().map(str::trim) {
        None | Some("") => WhisperBackend::Ct2,
        Some(s) => WhisperBackend::from_str(s).ok_or_else(|| {
            vm.new_value_error(
                "backend must be 'ct2' or 'burn' (same as xos.ai.whisper.CT2 / BURN)".to_string(),
            )
        })?,
    };
    let language: Option<String> = if let Some(v) = args.kwargs.get("language") {
        Some(v.clone().try_into_value::<String>(vm)?)
    } else {
        None
    };
    let listener_ptr_obj = mic_obj
        .get_attr("_listener_ptr", vm)
        .map_err(|_| vm.new_type_error("xos.audio.transcription expects xos.audio.Microphone".to_string()))?;
    let listener_ptr: usize = listener_ptr_obj.try_into_value(vm)?;
    if listener_ptr == 0 {
        return Err(vm.new_runtime_error("Invalid microphone pointer".to_string()));
    }

    let mut engine = TranscriptionEngine::new_with_size_backend_language(
        size.as_deref(),
        backend,
        language.as_deref(),
    )
    .map_err(|e| vm.new_value_error(e))?;
    let listener = unsafe { &*(listener_ptr as *const AudioListener) };
    engine.set_device_hint("python-mic", listener.buffer().sample_rate());

    let boxed = Box::new(PyTranscriber {
        listener_ptr,
        engine,
        last_yield_epoch: 0,
    });
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

    def transcribe(self, poll_interval=0.03):
        """
        One poll of the transcription engine. Returns ``(transcription, was_committed, is_new)``.

        ``transcription`` / ``was_committed`` come from ``_transcriber_transcribe_step`` (commits are
        rare in the baseline pipeline; use ``finish()`` / ``flush_live_to_stdout_commits`` for a
        final push). ``is_new`` is
        driven by the engine's ``transcript_epoch`` (no string equality): false when the epoch is
        unchanged since the last ``transcribe`` call. ``poll_interval`` is reserved; callers can
        sleep between polls (see ``record.py``).
        """
        import xos
        return xos.audio._transcriber_transcribe_step(self._ptr)

    def vad_prob(self):
        """Current Silero VAD speech probability [0, 1]."""
        import xos
        return xos.audio._transcriber_vad_prob(self._ptr)

    def buffered_seconds(self):
        """Approximate audio seconds currently buffered in the decode segment."""
        import xos
        return xos.audio._transcriber_buffered_seconds(self._ptr)

    def clip_cursor(self):
        """Advance decode cursor to current audio ingestion point (drop old segment backlog)."""
        import xos
        return xos.audio._transcriber_clip_cursor(self._ptr)

    def flush_commit(self):
        """Flush current live hypothesis to finalized commit(s), returning list[str]."""
        import xos
        return xos.audio._transcriber_flush_commit(self._ptr)

    def iterate(self, poll_interval=0.03):
        import xos
        while True:
            events = xos.audio._transcriber_next_events(self._ptr)
            if events:
                for ev in events:
                    yield ev
            else:
                xos.sleep(poll_interval)

    def finish(self):
        """Release transcriber state; returns any stdout commits flushed at shutdown (may be empty)."""
        if self._ptr != 0:
            import xos
            tail = xos.audio._transcriber_cleanup(self._ptr)
            self._ptr = 0
            return tail
        return []

    def __del__(self):
        self.finish()

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
    let buf = listener.buffer();
    let sr = buf.sample_rate();
    let ingested = buf.ingested_frame_count();
    state.engine.process_snapshot(sr, &channels, ingested);
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

pub fn transcriber_vad_prob(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let ptr: usize = args.bind(vm)?;
    let map = transcribers()
        .lock()
        .map_err(|_| vm.new_runtime_error("transcriber lock poisoned".to_string()))?;
    let state = map
        .get(&ptr)
        .ok_or_else(|| vm.new_runtime_error("Invalid transcriber pointer".to_string()))?;
    Ok(vm.ctx.new_float(state.engine.last_vad_speech_prob() as f64).into())
}

pub fn transcriber_buffered_seconds(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let ptr: usize = args.bind(vm)?;
    let map = transcribers()
        .lock()
        .map_err(|_| vm.new_runtime_error("transcriber lock poisoned".to_string()))?;
    let state = map
        .get(&ptr)
        .ok_or_else(|| vm.new_runtime_error("Invalid transcriber pointer".to_string()))?;
    Ok(vm.ctx.new_float(state.engine.buffered_segment_seconds() as f64).into())
}

pub fn transcriber_clip_cursor(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let ptr: usize = args.bind(vm)?;
    let mut map = transcribers()
        .lock()
        .map_err(|_| vm.new_runtime_error("transcriber lock poisoned".to_string()))?;
    let state = map
        .get_mut(&ptr)
        .ok_or_else(|| vm.new_runtime_error("Invalid transcriber pointer".to_string()))?;
    state.engine.clip_consumed_audio_cursor();
    Ok(vm.ctx.none())
}

pub fn transcriber_flush_commit(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let ptr: usize = args.bind(vm)?;
    let mut map = transcribers()
        .lock()
        .map_err(|_| vm.new_runtime_error("transcriber lock poisoned".to_string()))?;
    let state = map
        .get_mut(&ptr)
        .ok_or_else(|| vm.new_runtime_error("Invalid transcriber pointer".to_string()))?;
    state.engine.flush_live_to_stdout_commits();
    let out = state
        .engine
        .drain_stdout_commits()
        .into_iter()
        .map(|s| vm.ctx.new_str(s).into())
        .collect::<Vec<PyObjectRef>>();
    Ok(vm.ctx.new_list(out).into())
}

/// One transcription poll: returns `(transcription, was_committed, is_new)`.
///
/// `transcription` is normally the **live** caption string. `was_committed` is true when at least
/// one finalized line was queued to the iterator this poll (e.g. explicit flush), not during
/// ordinary partial streaming. `is_new` compares [`TranscriptionEngine::transcript_epoch`]
/// to the last seen value (bumped when the engine queues iterator events).
pub fn transcriber_transcribe_step(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let ptr: usize = args.bind(vm)?;
    let mut map = transcribers()
        .lock()
        .map_err(|_| vm.new_runtime_error("transcriber lock poisoned".to_string()))?;
    let state = map
        .get_mut(&ptr)
        .ok_or_else(|| vm.new_runtime_error("Invalid transcriber pointer".to_string()))?;
    let listener = unsafe { &*(state.listener_ptr as *const AudioListener) };
    let channels = listener.get_samples_by_channel();
    let buf = listener.buffer();
    let sr = buf.sample_rate();
    let ingested = buf.ingested_frame_count();
    state.engine.process_snapshot(sr, &channels, ingested);

    let events = state.engine.drain_iter_events();
    let mut new_commits: Vec<String> = Vec::new();
    let mut prev: Option<String> = None;
    for e in events {
        match e {
            None => {
                if let Some(t) = prev.take() {
                    let t = t.trim().to_string();
                    if !t.is_empty() {
                        new_commits.push(t);
                    }
                }
            }
            Some(s) => prev = Some(s),
        }
    }

    let was_committed = !new_commits.is_empty();
    let transcription = if was_committed {
        new_commits.join("\n")
    } else {
        state.engine.caption().trim().to_string()
    };

    let epoch = state.engine.transcript_epoch();
    let is_new = epoch != state.last_yield_epoch;
    state.last_yield_epoch = epoch;

    Ok(vm
        .ctx
        .new_tuple(vec![
            vm.ctx.new_str(transcription).into(),
            vm.ctx.new_bool(was_committed).into(),
            vm.ctx.new_bool(is_new).into(),
        ])
        .into())
}

pub fn transcriber_cleanup(args: FuncArgs, vm: &VirtualMachine) -> PyResult {
    let ptr: usize = args.bind(vm)?;
    if let Ok(mut set) = active_transcribers().lock() {
        set.remove(&ptr);
    }
    let mut tail_commits: Vec<String> = Vec::new();
    if let Ok(mut map) = transcribers().lock() {
        if let Some(mut state) = map.remove(&ptr) {
            let listener = unsafe { &*(state.listener_ptr as *const AudioListener) };
            let channels = listener.get_samples_by_channel();
            let buf = listener.buffer();
            let sr = buf.sample_rate();
            let ingested = buf.ingested_frame_count();
            state.engine.process_snapshot(sr, &channels, ingested);
            state.engine.flush_deferred_iter_delivery();
            state.engine.flush_live_to_stdout_commits();
            tail_commits = state.engine.drain_stdout_commits();
            let _ = state.engine.drain_iter_events();
        }
    }
    let items: Vec<PyObjectRef> = tail_commits
        .into_iter()
        .map(|s| vm.ctx.new_str(s).into())
        .collect();
    Ok(vm.ctx.new_list(items).into())
}

