use crate::engine::Application;

/// Python UI entry (`study::launcher`, `study.py`) for `xos app study`.
pub mod study;
/// Python UI entry (`whiteboard::launcher`, `whiteboard.py`) for `xos app whiteboard`.
pub mod whiteboard;
/// Text editor engine ([`text::TextApp`]) + Python UI entry (`text::launcher`, `text.py`).
pub mod text;
/// LAN remote viewer (`remote/launcher.rs`, `remote.py`) + desktop capture helpers (`remote/remote.rs`).
pub mod remote;
/// Implementation lives under [`transcription`]; the CLI subcommand is `transcribe` via [`transcribe`].
pub mod transcription;
#[cfg(target_arch = "wasm32")]
pub mod xpy;

/// `xos app text` / `xos app study` — Python windowed apps (`text.py`, `study.py`).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn maybe_python_cli_app(name: &str) -> Option<Box<dyn Application>> {
    match name {
        "text" => text::launcher::boxed_text_demo_app(),
        "study" => study::launcher::boxed_study_app(),
        "whiteboard" => whiteboard::launcher::boxed_whiteboard_app(),
        "remote" => remote::launcher::boxed_remote_app(),
        _ => None,
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn maybe_python_cli_app(name: &str) -> Option<Box<dyn Application>> {
    match name {
        "text" => text::launcher::boxed_text_demo_app(),
        "study" => study::launcher::boxed_study_app(),
        "remote" => remote::launcher::boxed_remote_app(),
        "xpy" => xpy::launcher::boxed_xpy_app(),
        _ => None,
    }
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
                    #[arg(long = "wasm", alias = "web")]
                    wasm: bool,
                    #[arg(long = "react-native")]
                    react_native: bool,
                    #[arg(long)]
                    ios: bool,
                },
            )*
            // Python windowed UI: `src/core/apps/text/text.py` via [`maybe_python_cli_app`] (`get_app("text")`).
            #[command(name = "text", about = "")]
            TextCli {
                #[arg(long = "wasm", alias = "web")]
                wasm: bool,
                #[arg(long = "react-native")]
                react_native: bool,
                #[arg(long)]
                ios: bool,
            },
            #[command(name = "study", about = "")]
            StudyCli {
                #[arg(long = "wasm", alias = "web")]
                wasm: bool,
                #[arg(long = "react-native")]
                react_native: bool,
                #[arg(long)]
                ios: bool,
            },
            #[command(name = "remote", about = "")]
            RemoteCli {
                #[arg(long = "wasm", alias = "web")]
                wasm: bool,
                #[arg(long = "react-native")]
                react_native: bool,
                #[arg(long)]
                ios: bool,
            },
            #[command(name = "whiteboard", about = "")]
            WhiteboardCli {
                #[arg(long = "wasm", alias = "web")]
                wasm: bool,
                #[arg(long = "react-native")]
                react_native: bool,
                #[arg(long)]
                ios: bool,
            },
        }

        pub fn run_app_command(app: AppCommands) {
            match app {
                $(
                    AppCommands::$Variant { wasm, react_native, ios } => {
                        if ios {
                            $crate::launch_ios_app(stringify!($file));
                        } else {
                            $crate::run_game(stringify!($file), wasm, react_native);
                        }
                    }
                )*
                AppCommands::TextCli { wasm, react_native, ios } => {
                    if ios {
                        $crate::launch_ios_app("text");
                    } else {
                        $crate::run_game("text", wasm, react_native);
                    }
                }
                AppCommands::StudyCli { wasm, react_native, ios } => {
                    if ios {
                        $crate::launch_ios_app("study");
                    } else {
                        $crate::run_game("study", wasm, react_native);
                    }
                }
                AppCommands::RemoteCli { wasm, react_native, ios } => {
                    if ios {
                        $crate::launch_ios_app("remote");
                    } else {
                        $crate::run_game("remote", wasm, react_native);
                    }
                }
                AppCommands::WhiteboardCli { wasm, react_native, ios } => {
                    if ios {
                        $crate::launch_ios_app("whiteboard");
                    } else {
                        $crate::run_game("whiteboard", wasm, react_native);
                    }
                }
            }
        }

        pub fn get_app(name: &str) -> Option<Box<dyn Application>> {
            if let Some(app) = $crate::apps::maybe_python_cli_app(name) {
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
            for extra in ["text", "study", "remote", "whiteboard"] {
                if !names.iter().any(|n| *n == extra) {
                    names.push(extra);
                }
            }
            #[cfg(target_arch = "wasm32")]
            if !names.iter().any(|n| *n == "xpy") {
                names.push("xpy");
            }
            names
        }
    };
}

define_apps! {
    Ball => ball::BallGame,
    Tracers => tracers::TracersApp,
    Camera => camera::CameraApp,
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
    IosRemote => ios_remote::IosRemoteApp,
    Mesh => mesh::MeshApp,
    Hang => hang::HangApp,
    Transcribe => transcribe::TranscribeApp,
    Vad => vad::VadApp,
}
