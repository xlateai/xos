use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock, RwLock};
use std::fmt::Debug;

/// Trait all tuneables implement
pub trait TuneableEntry: Send + Sync {
    fn file(&self) -> &'static str;
    fn line(&self) -> u32;
    fn column(&self) -> u32;
    fn name(&self) -> &'static str;
    fn write_source_line(&self) -> String;
}

static REGISTRY: OnceLock<Mutex<Vec<&'static dyn TuneableEntry>>> = OnceLock::new();

pub fn register(entry: &'static dyn TuneableEntry) {
    REGISTRY.get_or_init(Default::default).lock().unwrap().push(entry);
}

pub fn write_all_to_source() {
    let Some(registry) = REGISTRY.get() else { return };

    let mut by_file: HashMap<&'static str, Vec<&dyn TuneableEntry>> = HashMap::new();
    for entry in registry.lock().unwrap().iter() {
        by_file.entry(entry.file()).or_default().push(*entry);
    }

    for (file, entries) in by_file {
        let path = Path::new(file);
        let Ok(content) = fs::read_to_string(path) else { continue };
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

        for entry in entries {
            let idx = entry.line() as usize - 1;
            if idx < lines.len() {
                lines[idx] = entry.write_source_line();
            }
        }

        let _ = fs::write(path, lines.join("\n"));
    }
}

pub struct Tuneable<T: Copy + Debug + 'static> {
    name: &'static str,
    file: &'static str,
    line: u32,
    column: u32,
    value: RwLock<T>,
}

impl<T: Copy + Debug + 'static> Tuneable<T> {
    pub const fn new(name: &'static str, value: T, file: &'static str, line: u32, column: u32) -> Self {
        Self {
            name,
            file,
            line,
            column,
            value: RwLock::new(value),
        }
    }

    pub fn get(&self) -> T {
        *self.value.read().unwrap()
    }

    pub fn set(&self, v: T) {
        *self.value.write().unwrap() = v;
    }
}

macro_rules! impl_tuneable_entry {
    ($t:ty) => {
        impl TuneableEntry for Tuneable<$t> {
            fn file(&self) -> &'static str { self.file }
            fn line(&self) -> u32 { self.line }
            fn column(&self) -> u32 { self.column }
            fn name(&self) -> &'static str { self.name }
            fn write_source_line(&self) -> String {
                format!("    {}: {} = {:?};", self.name, stringify!($t), self.get())
            }
        }
    };
}

impl_tuneable_entry!(f32);
impl_tuneable_entry!(i32);
impl_tuneable_entry!(bool);
impl_tuneable_entry!(f64);
impl_tuneable_entry!(u32);

#[macro_export]
macro_rules! tuneables {
    ($($ident:ident : $ty:ty = $val:expr;)*) => {
        $(
            #[allow(non_snake_case)]
            pub fn $ident() -> &'static $crate::tuneable::Tuneable<$ty> {
                use std::sync::LazyLock;
                static INSTANCE: LazyLock<&'static $crate::tuneable::Tuneable<$ty>> = LazyLock::new(|| {
                    static INNER: $crate::tuneable::Tuneable<$ty> = $crate::tuneable::Tuneable::new(
                        stringify!($ident),
                        $val,
                        file!(),
                        line!(),
                        column!(),
                    );
                    $crate::tuneable::register(&INNER);
                    &INNER
                });
                *INSTANCE
            }
        )*
    };
}
