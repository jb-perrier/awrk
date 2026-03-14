mod app;
mod forms;
mod model;
mod schema_types;
mod value_editor;
mod worker;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> Result<(), eframe::Error> {
    env_logger::init();
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "awrk Explorer UI",
        options,
        Box::new(|_cc| Ok(Box::new(app::ExplorerUiApp::default()))),
    )
}
