extern crate gtk;
extern crate git2;
extern crate chrono;

mod ui;

use ui::init::InitViewable;
use ui::main::MainViewable;

pub struct Config;

impl Config {
    fn set_repo_dir(repo_dir: &str) {
        // TODO
    }

    fn repo_dir<'a>() -> Option<&'a str> {
        None
    }
}

fn main() {
    if gtk::init().is_err() {
        println!("Failed to initialize GTK.");
        return;
    }

    if let Some(repo_dir) = Config::repo_dir() {
        let repo = git2::Repository::open(&repo_dir).unwrap();
        let main_window = ui::main::MainWindow::with_repo(repo);
        main_window.show();
    } else {
        let init_window = ui::init::InitWindow::new();
        init_window.show();
    }

    gtk::main();
}
