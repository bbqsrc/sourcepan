use std::rc::{Rc, Weak};
use std::cell::RefCell;
use std::path::Path;
use std::fmt;

use git2;
use gtk::prelude::*;
use gtk;

use ui::Window;
use ui::main::{MainViewable, MainWindow};
use ui::AsMessageDialog;

struct InitPresenter<V: InitViewable> {
    view: RefCell<Weak<V>>
}

impl<V: InitViewable> InitPresenter<V> {
    fn new() -> InitPresenter<V> {
        InitPresenter { view: RefCell::new(Weak::new()) }
    }

    fn view(&self) -> Rc<V> {
        self.view.borrow()
            .upgrade()
            .expect("Presenter only running while view still exists")
    }

    fn click_open(&self) {
        self.view().open_repo_selector();
    }

    fn select_repo(&self, repo_dir: &Path) {
        use Config;

        let repo = match git2::Repository::open(&repo_dir) {
            Ok(repo) => repo,
            Err(err) => {
                self.view().handle_error(err);
                return;
            }
        };

        self.view().open_main_window_with(repo);

        Config::set_repo_dir(&repo_dir.to_string_lossy());
    }
}

pub trait InitViewable {
    fn new() -> Rc<Self>;
    fn show(&self);
    fn hide(&self);
    fn open_repo_selector(&self);
    fn open_main_window_with(&self, repo: git2::Repository);
    fn handle_error(&self, error: impl fmt::Display);
}

pub struct InitWindow {
    presenter: InitPresenter<InitWindow>,
    window: gtk::Window,
    main_window: RefCell<Option<Rc<MainWindow>>>
}

impl InitWindow {
    fn create() -> (gtk::Window, gtk::Button) {
        let window = gtk::Window::new(gtk::WindowType::Toplevel);
        window.set_title("Sourcepan");
        window.set_default_size(600, 64);

        let open_button = gtk::Button::new_with_label("Open");
        window.add(&open_button);

        window.connect_delete_event(|_, _| {
            gtk::main_quit();
            Inhibit(false)
        });

        (window, open_button)
    }
}

impl Window for InitWindow {}

impl InitViewable for InitWindow {
    fn new() -> Rc<InitWindow> {
        let (window, open_button) = InitWindow::create();

        let view = view!(InitWindow {
            presenter: InitPresenter::new(),
            window: window,
            main_window: RefCell::new(None)
        });

        open_button.connect_clicked(weak!(view => move |_| {
            if let Some(v) = view.upgrade() {
                v.presenter.click_open();
            } else {
                panic!("Open button on InitWindow failed to resolve weak reference");
            }
        }));

        view
    }

    fn handle_error(&self, error: impl fmt::Display) {
        let dialog = error.as_message_dialog(Some(&self.window));
        dialog.run();
        dialog.destroy();
    }

    fn show(&self) {
        self.window.show_all();
    }

    fn hide(&self) {
        self.window.hide();
    }

    fn open_repo_selector(&self) {
        let dialog = gtk::FileChooserNative::new(
            Some("Select Repository"),
            Some(&self.window),
            gtk::FileChooserAction::SelectFolder,
            Some("_Open"),
            Some("_Cancel"));
        
        let result = dialog.run();

        if result == -3 {
            if let Some(filename) = dialog.get_filename() {
                self.presenter.select_repo(&filename);
            }
        }
    }

    fn open_main_window_with(&self, repo: git2::Repository) {
        self.hide();
        let main_window = MainWindow::with_repo(repo);
        main_window.show();

        // TODO: remove this terrible hack; use a window mgr
        *self.main_window.borrow_mut() = Some(main_window);
    }
}
