// Sourcepan - a Gtk+ Git client written in Rust
// Copyright (C) 2018  Brendan Molloy <brendan@bbqsrc.net>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 3 as 
// published by the Free Software Foundation.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

extern crate gtk;
extern crate git2;
extern crate chrono;
extern crate preferences;
extern crate notify;
extern crate glib;
extern crate gdk;
extern crate pango;

use std::rc::Rc;

use preferences::{AppInfo, Preferences, PreferencesMap};
use gtk::prelude::*;

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

fn create_main_window(repo: git2::Repository) -> Result<Rc<ui::main::MainWindow>, ui::main::MainWindowError> {
    let main_window = ui::main::MainWindow::with_repo(repo)?;
    main_window.show();
    Ok(main_window)
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

    let css_provider = gtk::CssProvider::new();
    match css_provider.load_from_data(include_str!("ui/app.css").as_bytes()) {
        Ok(_) => {},
        Err(e) => panic!(e)
    };
    gtk::StyleContext::add_provider_for_screen(
        &gdk::Screen::get_default().unwrap(),
        &css_provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

    // Holds a strong reference to the primary window to stop some *fun* UB
    let _window: Rc<ui::Window> = if let Some(repo_dir) = Config::repo_dir() {
        match git2::Repository::open(&repo_dir) {
            Ok(repo) => match create_main_window(repo) {
                Ok(w) => w,
                Err(_) => create_init_window()
            }
            Err(_) => create_init_window()
        }
    } else {
        create_init_window()
    };

    gtk::main();
}
