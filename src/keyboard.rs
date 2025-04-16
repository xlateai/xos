#[derive(Debug, Clone)]
pub struct Key {
    pub is_pressed: bool,
    pub is_held: bool,
    pub is_released: bool,
    pub num_ticks_held: u32,
}

impl Key {
    pub fn new() -> Self {
        Self {
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
        $( $field:ident => $label:expr ),* $(,)?
    ) => {
        #[derive(Debug, Clone)]
        pub struct KeyboardKeys {
            $( pub $field: Key, )*
        }

        impl KeyboardKeys {
            pub fn new() -> Self {
                Self {
                    $( $field: Key::new(), )*
                }
            }

            pub fn all_keys_mut(&mut self) -> Vec<(&'static str, &mut Key)> {
                vec![ $( ($label, &mut self.$field), )* ]
            }

            pub fn all_keys(&self) -> Vec<(&'static str, &Key)> {
                vec![ $( ($label, &self.$field), )* ]
            }
        }
    };
}

define_keyboard_keys! {
    a => "a", b => "b", c => "c", d => "d", e => "e", f => "f", g => "g",
    h => "h", i => "i", j => "j", k => "k", l => "l", m => "m", n => "n",
    o => "o", p => "p", q => "q", r => "r", s => "s", t => "t", u => "u",
    v => "v", w => "w", x => "x", y => "y", z => "z",
    zero => "0", one => "1", two => "2", three => "3", four => "4",
    five => "5", six => "6", seven => "7", eight => "8", nine => "9",
    percent => "%", star => "*", dash => "-", underscore => "_", plus => "+",
    equals => "=", slash => "/", backslash => "\\", comma => ",", period => ".",
    space => " ", tab => "\t", enter => "\n", backspace => "\u{8}",
    shift => "shift", control => "control", alt => "alt", meta => "meta",
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
        for (label, key) in self.keys.all_keys_mut() {
            key.update(active.contains(&label));
        }
    }

    pub fn get_pressed_keys(&self) -> Vec<&str> {
        self.keys.all_keys()
            .into_iter()
            .filter(|(_, k)| k.is_pressed)
            .map(|(k, _)| k)
            .collect()
    }

    pub fn get_held_keys(&self) -> Vec<&str> {
        self.keys.all_keys()
            .into_iter()
            .filter(|(_, k)| k.is_held)
            .map(|(k, _)| k)
            .collect()
    }

    pub fn get_released_keys(&self) -> Vec<&str> {
        self.keys.all_keys()
            .into_iter()
            .filter(|(_, k)| k.is_released)
            .map(|(k, _)| k)
            .collect()
    }
}

// fn main() {
//     let mut keyboard = Keyboard::new();

//     keyboard.tick(&["a", "shift", "8", "*"]);

//     for key in keyboard.get_pressed_keys() {
//         println!("Key pressed: {}", key);
//     }

//     for key in keyboard.get_held_keys() {
//         println!("Key held: {}", key);
//     }

//     for key in keyboard.get_released_keys() {
//         println!("Key released: {}", key);
//     }

//     if keyboard.keys.a.is_held {
//         println!("IntelliSense works: 'a' is held!");
//     }
// }