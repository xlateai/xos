use crate::engine::Application;

/// Implementation lives under [`transcription`]; the CLI subcommand is `transcribe` via [`transcribe`].
pub mod transcription;
/// Text editor engine ([`text::TextApp`]) + Python UI entry (`text::launcher`, `text.py`).
pub mod text;
/// Python UI entry (`study::launcher`, `study.py`) for `xos app study`.
pub mod study;

/// `xos app text` / `xos app study` — Python windowed apps (`text.py`, `study.py`).
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn maybe_python_cli_app(name: &str) -> Option<Box<dyn Application>> {
    match name {
        "text" => text::launcher::boxed_text_demo_app(),
        "study" => study::launcher::boxed_study_app(),
        _ => None,
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn maybe_python_cli_app(_name: &str) -> Option<Box<dyn Application>> {
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
            }
        }

        pub fn get_app(name: &str) -> Option<Box<dyn Application>> {
            if let Some(app) = $crate::apps::maybe_python_cli_app(name) {
                return Some(app);
            }
            #[cfg(target_arch = "wasm32")]
            if name == "text" {
                let mut app = $crate::apps::text::TextApp::new();
                let text = "xos text wasm runtime\n\ncompiled with `xos compile --wasm`\nlaunched with `xos app text --wasm`";
                app.text_rasterizer.set_text(text.to_string());
                app.cursor_position = text.chars().count();
                return Some(Box::new(app));
            }
            #[cfg(target_arch = "wasm32")]
            if name == "study" {
                let mut app = $crate::apps::text::TextApp::new();
                let text = "xos study wasm runtime\n\nThe native study app is Python/data-backed today, so this browser build is a placeholder until Python app launching is wired for wasm.\n\nTry `xos app text --wasm` or any Rust app name to exercise the wasm renderer.";
                app.text_rasterizer.set_text(text.to_string());
                app.cursor_position = text.chars().count();
                return Some(Box::new(app));
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
            for extra in ["text", "study"] {
                if !names.iter().any(|n| *n == extra) {
                    names.push(extra);
                }
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
