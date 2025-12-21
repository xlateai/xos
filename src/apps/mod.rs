use crate::engine::Application;

#[macro_export]
macro_rules! define_apps {
    ( $( $Variant:ident => $file:ident :: $Struct:ident ),* $(,)? ) => {
        $(
            pub mod $file;
        )*

        #[derive(clap::Subcommand)]
        pub enum AppCommands {
            $(
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
            }
        }

        pub fn get_app(name: &str) -> Option<Box<dyn Application>> {
            match name {
                $(
                    stringify!($file) => Some(Box::new($file::$Struct::new())),
                )*
                _ => None,
            }
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
    Text => text::TextApp,
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
}
