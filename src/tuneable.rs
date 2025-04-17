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

#[macro_export]
macro_rules! tuneable {
    ($name:literal => $val:literal) => {
        Tuneable::Editable {
            name: $name,
            value: $val,
            file: file!(),
            line: line!(),
            column: column!(),
        }
    };
}

pub use crate::tuneable;