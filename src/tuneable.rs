use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub enum Tuneable<T> {
    Static(T),
    Editable {
        name: &'static str,
        value: T,
        file: &'static str,
        line: u32,
        column: u32,
    },
}

impl<T: Copy> Tuneable<T> {
    pub fn get(&self) -> T {
        match self {
            Tuneable::Static(v) => *v,
            Tuneable::Editable { value, .. } => *value,
        }
    }

    pub fn set(&mut self, new_value: T) {
        if let Tuneable::Editable { value, .. } = self {
            *value = new_value;
        }
    }
}

impl Tuneable<f32> {
    /// Attempts to update the corresponding literal in the source file
    pub fn try_write_update_pair(
        a: *const Tuneable<f32>,
        b: *const Tuneable<f32>,
        new_a: f32,
        new_b: f32,
    ) {
        unsafe {
            match (&*a, &*b) {
                (
                    Tuneable::Editable { file, line, .. },
                    Tuneable::Editable { .. },
                ) => {
                    let path = Path::new(file);
                    let contents = fs::read_to_string(path).expect("could not read file");
    
                    let mut lines: Vec<String> = contents.lines().map(|l| l.to_string()).collect();
                    let start = *line as usize - 1;
    
                    if start + 1 >= lines.len() {
                        eprintln!("tuneable: line out of bounds");
                        return;
                    }
    
                    if lines[start].contains("square_x") && lines[start + 1].contains("square_y") {
                        lines[start] = format!("    tuneable!(\"square_x\" => {:.1}),", new_a);
                        lines[start + 1] = format!("    tuneable!(\"square_y\" => {:.1}),", new_b);
    
                        fs::write(path, lines.join("\n")).expect("could not write updated file");
                    } else {
                        eprintln!("tuneable: expected square_x/square_y lines not found");
                    }
                }
                _ => {}
            }
        }
    }
    
}

#[macro_export]
macro_rules! tuneable {
    ($name:literal => $val:literal) => {
        $crate::tuneable::Tuneable::Editable {
            name: $name,
            value: $val,
            file: file!(),
            line: line!(),
            column: column!(),
        }
    };
}

pub use crate::tuneable;
