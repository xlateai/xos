#[macro_export]
macro_rules! define_apps {
    ( $( $name:ident => $string:literal ),* $(,)? ) => {
        #[derive(clap::Subcommand)]
        pub enum AppCommands {
            $(
                #[allow(non_camel_case_types)]
                $name {
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
                    AppCommands::$name { web, react_native } => {
                        $crate::run_game($string, web, react_native);
                    }
                )*
            }
        }
    };
}
