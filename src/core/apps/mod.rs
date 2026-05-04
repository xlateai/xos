use crate::engine::Application;

/// Implementation lives under [`transcription`]; the CLI subcommand is `transcribe` via [`transcribe`].
pub mod transcription;
/// Text editor engine ([`text::TextApp`]) + Python UI entry (`text::launcher`, `text.py`).
pub mod text;

/// `xos app text` / iOS `text`: Python UI (`src/core/apps/text/text.py`) instead of a Rust-only shell.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn maybe_python_text_demo(name: &str) -> Option<Box<dyn Application>> {
    if name != "text" {
        return None;
    }
    text::launcher::boxed_text_demo_app()
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn maybe_python_text_demo(_name: &str) -> Option<Box<dyn Application>> {
    None
}

#[macro_export]
macro_rules! define_apps {
    ( $( $Variant:ident => $file:ident :: $Struct:ident ),* $(,)? ) => {
        $(
            pub mod $file;
        )*

        #[derive(clap::Subcommand)]
        pub enum AppCommands {
            $(
                // Intentionally no `///` doc: the macro repeats one string for every variant and
                // would spam `xos rs --help`. Subcommand names are `src/core/apps/<file>.rs`.
                #[command(name = stringify!($file), about = "")]
                #[allow(non_camel_case_types)]
                $Variant {
                    #[arg(long)]
                    web: bool,
                    #[arg(long = "react-native")]
                    react_native: bool,
                    #[arg(long)]
                    ios: bool,
                },
            )*
            // Python windowed UI: `src/core/apps/text/text.py` via [`maybe_python_text_demo`] (`get_app("text")`).
            #[command(name = "text", about = "")]
            TextCli {
                #[arg(long)]
                web: bool,
                #[arg(long = "react-native")]
                react_native: bool,
                #[arg(long)]
                ios: bool,
            },
        }

        pub fn run_app_command(app: AppCommands) {
            match app {
                $(
                    AppCommands::$Variant { web, react_native, ios } => {
                        if ios {
                            $crate::launch_ios_app(stringify!($file));
                        } else {
                            $crate::run_game(stringify!($file), web, react_native);
                        }
                    }
                )*
                AppCommands::TextCli { web, react_native, ios } => {
                    if ios {
                        $crate::launch_ios_app("text");
                    } else {
                        $crate::run_game("text", web, react_native);
                    }
                }
            }
        }

        pub fn get_app(name: &str) -> Option<Box<dyn Application>> {
            if let Some(app) = $crate::apps::maybe_python_text_demo(name) {
                return Some(app);
            }
            match name {
                $(
                    stringify!($file) => Some(Box::new($file::$Struct::new())),
                )*
                _ => None,
            }
        }

        /// Get a list of all available app names
        pub fn list_apps() -> Vec<&'static str> {
            let mut names = vec![
                $(
                    stringify!($file),
                )*
            ];
            if !names.iter().any(|n| *n == "text") {
                names.push("text");
            }
            names
        }
    };
}

define_apps! {
    Ball => ball::BallGame,
    Tracers => tracers::TracersApp,
    Camera => camera::CameraApp,
    Whiteboard => whiteboard::Whiteboard,
    Blank => blank::BlankApp,
    Crash => crash::CrashApp,
    Waveform => waveform::Waveform,
    Scroll => scroll::ScrollApp,
    Wireframe => wireframe::WireframeDemo,
    WireframeText => wireframe_text::WireframeText,
    Triangles => triangles::TrianglesApp,
    Cursor => cursor::CursorApp,
    Audiovis => audiovis::AudiovisApp,
    AudioEdit => audioeditor::AudioEditApp,
    Partitions => partitions::Partitions,
    Coder => coder::CoderApp,
    Leds => leds::Leds,
    IosSensors => ios_sensors::IosSensorsApp,
    AudioRelay => audio_relay::AudioRelay,
    TextMesh => text_mesh::TextMeshApp,
    Overlay => overlay::OverlayApp,
    Remote => remote::RemoteApp,
    IosRemote => ios_remote::IosRemoteApp,
    Mesh => mesh::MeshApp,
    Hang => hang::HangApp,
    Transcribe => transcribe::TranscribeApp,
    Vad => vad::VadApp,
}
