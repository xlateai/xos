use xos_core::engine::Application;

pub mod python_apps;
#[cfg(target_arch = "wasm32")]
pub mod xpy_wasm;

pub mod transcription;
pub mod text;
pub mod remote;
pub mod mesh;

#[derive(Debug, Clone, Copy)]
pub struct AppLaunchFlags {
    pub wasm: bool,
    pub react_native: bool,
    pub ios: bool,
}

/// Launch a native Rust app (`xos rs-app <name>`).
pub fn run_rs_app_by_name(name: &str, flags: AppLaunchFlags) {
    if flags.ios {
        crate::launch_ios_app(name);
        return;
    }
    crate::run_game(name, flags.wasm, flags.react_native);
}

/// Launch a Python app from `src/apps/<name>/<name>.py` (`xos app <name>`).
pub fn run_python_app_by_name(name: &str, flags: AppLaunchFlags) {
    if flags.react_native {
        eprintln!("❌ python apps under src/apps/ do not support --react-native yet");
        std::process::exit(1);
    }
    if flags.ios {
        if let Err(e) = python_apps::stage_python_app_for_ios(name, &native_app_names()) {
            eprintln!("❌ {e}");
            std::process::exit(1);
        }
        crate::launch_ios_app(name);
        return;
    }
    if flags.wasm {
        #[cfg(not(target_arch = "wasm32"))]
        {
            python_apps::launch_python_app_wasm(name, &native_app_names(), &[]);
            return;
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = name;
            eprintln!("❌ launch python apps from the native `xos` CLI with `--wasm`");
            std::process::exit(1);
        }
    }
    python_apps::run_python_app(name, &native_app_names());
}

pub fn list_python_app_names() -> Result<Vec<String>, String> {
    python_apps::python_app_names(&native_app_names())
}

#[macro_export]
macro_rules! define_apps {
    ( $( $Variant:ident => $file:ident :: $Struct:ident ),* $(,)? ) => {
        $(
            pub mod $file;
        )*

        pub fn native_app_names() -> Vec<&'static str> {
            vec![ $( stringify!($file), )* ]
        }

        #[derive(clap::Subcommand)]
        pub enum RsAppCommands {
            $(
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
        }

        pub fn run_rs_app_command(app: RsAppCommands) {
            match app {
                $(
                    RsAppCommands::$Variant { wasm, react_native, ios } => {
                        run_rs_app_by_name(
                            stringify!($file),
                            AppLaunchFlags { wasm, react_native, ios },
                        );
                    }
                )*
            }
        }

        pub fn get_native_app(name: &str) -> Option<Box<dyn Application>> {
            match name {
                $(
                    stringify!($file) => Some(Box::new($file::$Struct::new())),
                )*
                _ => None,
            }
        }

        pub fn get_app(name: &str) -> Option<Box<dyn Application>> {
            if let Some(app) = python_apps::boxed_python_app(name, &native_app_names()) {
                return Some(app);
            }
            #[cfg(target_arch = "wasm32")]
            if name == "xpy" {
                if let Some(app) = xpy_wasm::boxed_xpy_app() {
                    return Some(app);
                }
            }
            get_native_app(name)
        }

        pub fn list_apps() -> Vec<String> {
            let mut names: Vec<String> = vec![ $( stringify!($file).to_string(), )* ];
            if let Ok(py) = list_python_app_names() {
                for n in py {
                    if !names.iter().any(|x| x == &n) {
                        names.push(n);
                    }
                }
            }
            #[cfg(target_arch = "wasm32")]
            if !names.iter().any(|n| n == "xpy") {
                names.push("xpy".to_string());
            }
            names.sort();
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
    Hang => hang::HangApp,
    Transcribe => transcribe::TranscribeApp,
    Vad => vad::VadApp,
}
