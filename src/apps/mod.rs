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
            /// Run an app from a local file or folder path
            Dev {
                #[arg()]
                path: String,
            },
        }

        pub fn run_app_command(app: AppCommands) {
            match app {
                $(
                    AppCommands::$Variant { web, react_native } => {
                        $crate::run_game(stringify!($file), web, react_native);
                    }
                )*
                AppCommands::Dev { path } => {
                    match std::fs::canonicalize(&path) {
                        Ok(abs) => {
                            println!("📂 Launching app from: {}", abs.display());
                            // TODO: load and run app from .rs or .py file, etc.
                        }
                        Err(_) => {
                            eprintln!("❗ Failed to resolve path: {}\n", path);
                            eprintln!("👉 You can point to a code file/folder, OR use one of these apps:");
                            $(
                                eprintln!("- {}", stringify!($file));
                            )*
                        }
                    }
                }
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
    Waveform => waveform::Waveform,
    Scroll => scroll::ScrollApp,
    Text => text::TextApp,
    Wireframe => wireframe::WireframeDemo,
    WireframeText => wireframe_text::WireframeText,
    Triangles => triangles::TrianglesApp,
}
