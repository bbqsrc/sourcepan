extern crate gtk;
extern crate git2;
extern crate chrono;
extern crate preferences;

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
