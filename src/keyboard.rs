#[derive(Debug, Clone)]
pub struct Key {
    pub label: &'static str,
    pub char: char,
    pub is_pressed: bool,
    pub is_held: bool,
    pub is_released: bool,
    pub num_ticks_held: u32,
}

impl Key {
    pub fn new(label: &'static str, ch: char) -> Self {
        Self {
            label,
            char: ch,
            is_pressed: false,
            is_held: false,
            is_released: false,
            num_ticks_held: 0,
        }
    }

    pub fn update(&mut self, active: bool) {
        let was_held = self.is_held;
        self.is_held = active;
        self.is_pressed = active && !was_held;
        self.is_released = !active && was_held;
        if self.is_held {
            self.num_ticks_held += 1;
        } else {
            self.num_ticks_held = 0;
        }
    }
}

#[macro_export]
macro_rules! define_keyboard_keys {
    (
        $( $field:ident => ($label:expr, $ch:expr) ),* $(,)?
    ) => {
        #[derive(Debug, Clone)]
        pub struct KeyboardKeys {
            $( pub $field: Key, )*
        }

        impl KeyboardKeys {
            pub fn new() -> Self {
                Self {
                    $( $field: Key::new($label, $ch), )*
                }
            }

            pub fn all_keys_mut(&mut self) -> Vec<&mut Key> {
                vec![ $( &mut self.$field, )* ]
            }

            pub fn all_keys(&self) -> Vec<&Key> {
                vec![ $( &self.$field, )* ]
            }
        }
    };
}

define_keyboard_keys! {
    a => ("a", 'a'), b => ("b", 'b'), c => ("c", 'c'), d => ("d", 'd'),
    e => ("e", 'e'), f => ("f", 'f'), g => ("g", 'g'), h => ("h", 'h'),
    i => ("i", 'i'), j => ("j", 'j'), k => ("k", 'k'), l => ("l", 'l'),
    m => ("m", 'm'), n => ("n", 'n'), o => ("o", 'o'), p => ("p", 'p'),
    q => ("q", 'q'), r => ("r", 'r'), s => ("s", 's'), t => ("t", 't'),
    u => ("u", 'u'), v => ("v", 'v'), w => ("w", 'w'), x => ("x", 'x'),
    y => ("y", 'y'), z => ("z", 'z'),

    zero => ("0", '0'), one => ("1", '1'), two => ("2", '2'), three => ("3", '3'),
    four => ("4", '4'), five => ("5", '5'), six => ("6", '6'),
    seven => ("7", '7'), eight => ("8", '8'), nine => ("9", '9'),

    percent => ("%", '%'), star => ("*", '*'), dash => ("-", '-'),
    underscore => ("_", '_'), plus => ("+", '+'), equals => ("=", '='),
    slash => ("/", '/'), backslash => ("\\", '\\'), comma => (",", ','),
    period => (".", '.'),

    space => ("space", ' '), tab => ("tab", '\t'), enter => ("enter", '\n'),
    backspace => ("backspace", '\u{8}'),

    shift => ("shift", '\0'), control => ("control", '\0'),
    alt => ("alt", '\0'), meta => ("meta", '\0'),
}

#[derive(Debug, Clone)]
pub struct KeyboardState {
    pub keys: KeyboardKeys,
}

impl KeyboardState {
    pub fn new() -> Self {
        Self {
            keys: KeyboardKeys::new(),
        }
    }

    pub fn tick(&mut self, active: &[&str]) {
        for key in self.keys.all_keys_mut() {
            key.update(active.contains(&key.label));
        }
    }

    pub fn get_pressed_keys(&self) -> Vec<&Key> {
        self.keys.all_keys()
            .into_iter()
            .filter(|k| k.is_pressed)
            .collect()
    }

    pub fn get_held_keys(&self) -> Vec<&Key> {
        self.keys.all_keys()
            .into_iter()
            .filter(|k| k.is_held)
            .collect()
    }

    pub fn get_released_keys(&self) -> Vec<&Key> {
        self.keys.all_keys()
            .into_iter()
            .filter(|k| k.is_released)
            .collect()
    }
}
