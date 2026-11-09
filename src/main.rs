mod algorithm;
mod anthem;
mod audio;
mod config;
mod input;
mod tui;
mod visualizer;

fn main() {
    if let Err(error) = tui::run() {
        eprintln!("Mobilization Sort завершился с ошибкой: {error}");
        std::process::exit(1);
    }
}
