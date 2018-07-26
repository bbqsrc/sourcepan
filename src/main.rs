extern crate gtk;
extern crate git2;
extern crate chrono;

mod ui;

use ui::init::InitViewable;

fn main() {
    if gtk::init().is_err() {
        println!("Failed to initialize GTK.");
        return;
    }

    let init_window = ui::init::InitWindow::new();
    init_window.show();

    gtk::main();
}
