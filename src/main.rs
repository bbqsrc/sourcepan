extern crate gtk;
extern crate git2;
extern crate chrono;
extern crate preferences;
extern crate notify;
extern crate glib;
extern crate gdk;

use std::rc::Rc;

use preferences::{AppInfo, Preferences, PreferencesMap};

mod ui;

use ui::init::InitViewable;
use ui::main::MainViewable;

const APP_INFO: AppInfo = AppInfo { name: "Sourcepan", author: "Brendan Molloy" };

pub struct Config;

impl Config {
    fn set_repo_dir(repo_dir: &str) {
        let mut map: PreferencesMap<String> = PreferencesMap::new();
        map.insert("repo_dir".into(), repo_dir.into());
        map.save(&APP_INFO, "app").unwrap();
    }

    fn repo_dir() -> Option<String> {
        let map = PreferencesMap::<String>::load(&APP_INFO, "app");

        match map {
            Ok(m) => Some(m["repo_dir"].clone()),
            Err(_) => None
        }
    }
}

fn create_main_window(repo: git2::Repository) -> Rc<ui::main::MainWindow> {
    let main_window = ui::main::MainWindow::with_repo(repo);
    main_window.show();
    main_window
}

fn create_init_window() -> Rc<ui::init::InitWindow> {
    let init_window = ui::init::InitWindow::new();
    init_window.show();
    init_window
}

fn main() {
    if gtk::init().is_err() {
        println!("Failed to initialize GTK.");
        return;
    }

    // Holds a strong reference to the primary window to stop some *fun* UB
    let _window: Rc<ui::Window> = if let Some(repo_dir) = Config::repo_dir() {
        match git2::Repository::open(&repo_dir) {
            Ok(repo) => create_main_window(repo),
            Err(_) => create_init_window()
        }
    } else {
        create_init_window()
    };

    gtk::main();
}
