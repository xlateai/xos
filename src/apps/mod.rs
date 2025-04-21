// src/apps/mod.rs

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
                },
            )*
        }

        pub fn run_app_command(app: AppCommands) {
            match app {
                $(
                    AppCommands::$Variant { web, react_native } => {
                        $crate::run_game(stringify!($file), web, react_native);
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

// Expand the macro here
define_apps! {
    Ball => ball::BallGame,
    Tracers => tracers::TracersApp,
    Camera => camera::CameraApp,
    Whiteboard => whiteboard::Whiteboard,
    Blank => blank::BlankApp,
    Waveform => waveform::Waveform,
    Scroll => scroll::ScrollApp,
    Text => text::TextApp,
    Wireframe => wireframe::WireframeDemo,
    WireframeText => wireframe_text::WireframeText,
    Triangles => triangles::TrianglesApp,
}
